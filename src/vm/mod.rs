//! Ferrodock's own Docker engine: a small Linux system with the Docker
//! engine inside, so nothing else needs to be installed (#114).
//!
//! - On Apple Silicon Macs it's a VM, through Apple's Virtualization
//!   framework. Everything lives in `~/.ferrodock`, and the engine's
//!   socket is `~/.ferrodock/docker.sock`.
//! - On Windows it's a WSL distro called `ferrodock`, kept in
//!   `%LOCALAPPDATA%\Ferrodock`. Ferrodock carries the engine's socket
//!   over the distro's standard input and output, and offers it as the
//!   named pipe `\\.\pipe\ferrodock_engine`.
//!
//! The image (built in this repository's `engine/`) is downloaded on
//! first use.

#[cfg(any(engine, test))]
pub mod download;
#[cfg(all(engine, target_os = "macos"))]
mod macos;
#[cfg(all(engine, target_os = "macos"))]
use macos as platform;
#[cfg(any(all(engine, windows), test))]
mod mux;
#[cfg(all(engine, windows))]
mod windows;
#[cfg(any(all(engine, windows), test))]
mod wsl;
#[cfg(all(engine, windows))]
use windows as platform;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// How the engine shows up in the engine list.
pub const NAME: &str = "Ferrodock";
/// The virtiofs tag the home folder is shared under.
#[cfg(all(engine, target_os = "macos"))]
pub const SHARE_TAG: &str = "home";
/// Where the agent in the VM offers the engine's socket.
#[cfg(all(engine, target_os = "macos"))]
pub const VSOCK_PORT: u32 = 2375;

/// Whether this machine can run the engine.
pub const SUPPORTED: bool = cfg!(engine);

/// Where everything is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    pub dir: PathBuf,
    /// The unpacked image: `kernel` and `rootfs.img`.
    pub image: PathBuf,
    pub kernel: PathBuf,
    pub rootfs: PathBuf,
    /// Images, containers and volumes.
    pub data: PathBuf,
    pub socket: PathBuf,
    /// The WSL distro's disk and files (Windows).
    pub distro: PathBuf,
    /// The engine's log: the agent's and the kernel's messages.
    pub console: PathBuf,
}

impl Paths {
    pub fn under(dir: PathBuf) -> Self {
        let image = dir.join("engine");
        Self {
            kernel: image.join("kernel"),
            rootfs: image.join("rootfs.img"),
            data: dir.join("data.img"),
            socket: dir.join("docker.sock"),
            distro: dir.join("wsl"),
            console: dir.join("console.log"),
            image,
            dir,
        }
    }

    pub fn default() -> Option<Self> {
        if cfg!(windows) {
            return std::env::var_os("LOCALAPPDATA")
                .map(|dir| Self::under(PathBuf::from(dir).join("Ferrodock")));
        }
        std::env::var_os("HOME").map(|home| Self::under(PathBuf::from(home).join(".ferrodock")))
    }

    /// Whether the engine is set up: the image downloaded, or on Windows
    /// the distro imported into WSL.
    pub fn installed(&self) -> bool {
        if cfg!(windows) {
            return self.distro.join("ext4.vhdx").is_file();
        }
        self.kernel.is_file() && self.rootfs.is_file()
    }

    /// The Docker host clients connect to.
    pub fn host(&self) -> String {
        if cfg!(windows) {
            return PIPE.into();
        }
        format!("unix://{}", self.socket.display())
    }
}

/// The named pipe Ferrodock offers the engine on, on Windows.
pub const PIPE: &str = "npipe:////./pipe/ferrodock_engine";

/// What the VM gets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Resources {
    pub cpus: u32,
    pub memory_mib: u64,
}

impl Default for Resources {
    /// Half the CPUs (2 to 8) and 4 GB: enough for typical development
    /// stacks; unused memory goes back to macOS.
    fn default() -> Self {
        let cpus = std::thread::available_parallelism().map_or(4, |n| n.get() as u32);
        Self {
            cpus: (cpus / 2).clamp(2, 8),
            memory_mib: 4096,
        }
    }
}

/// Where the engine is at.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(not(engine), allow(dead_code))]
pub enum State {
    NotInstalled,
    Downloading { done: u64, total: Option<u64> },
    Stopped,
    Starting,
    Running,
    Failed(String),
}

impl State {
    pub fn busy(&self) -> bool {
        matches!(self, State::Downloading { .. } | State::Starting)
    }
}

/// The engine, driven from the UI. Long work (downloading, booting) runs on
/// its own thread; `state()` says how it goes.
pub struct Engine {
    paths: Option<Paths>,
    state: Arc<Mutex<State>>,
    #[cfg(engine)]
    running: Arc<Mutex<Option<platform::Running>>>,
}

impl Engine {
    pub fn new() -> Self {
        let paths = Paths::default();
        let state = match &paths {
            Some(p) if p.installed() => State::Stopped,
            _ => State::NotInstalled,
        };
        Self {
            paths,
            state: Arc::new(Mutex::new(state)),
            #[cfg(engine)]
            running: Arc::default(),
        }
    }

    pub fn state(&self) -> State {
        self.state
            .lock()
            .map(|s| s.clone())
            .unwrap_or(State::Stopped)
    }

    fn set(state: &Mutex<State>, value: State) {
        if let Ok(mut s) = state.lock() {
            *s = value;
        }
    }

    /// Downloads the image if needed, then boots the engine. `repaint`
    /// wakes the UI as things change.
    pub fn start(&self, resources: Resources, repaint: impl Fn() + Send + 'static) {
        let Some(paths) = self.paths.clone() else {
            Self::set(&self.state, State::Failed("No home folder".into()));
            return;
        };
        if self.state().busy() || self.state() == State::Running {
            return;
        }
        if !SUPPORTED {
            Self::set(
                &self.state,
                State::Failed(
                    "Ferrodock's engine needs a Mac with Apple Silicon or Windows".into(),
                ),
            );
            return;
        }
        let state = self.state.clone();
        #[cfg(engine)]
        let running = self.running.clone();
        std::thread::spawn(move || {
            let result = (|| {
                std::fs::create_dir_all(&paths.dir).map_err(|e| e.to_string())?;
                #[cfg(engine)]
                {
                    if !paths.installed() {
                        let archive = download::fetch(|done, total| {
                            Self::set(&state, State::Downloading { done, total });
                            repaint();
                        })?;
                        Self::set(&state, State::Starting);
                        repaint();
                        platform::install(&paths, &archive)?;
                    }
                    Self::set(&state, State::Starting);
                    repaint();
                    let engine = platform::Running::start(&paths, &resources)?;
                    if let Ok(mut slot) = running.lock() {
                        *slot = Some(engine);
                    }
                    Ok::<(), String>(())
                }
                #[cfg(not(engine))]
                {
                    let _ = resources;
                    Err("unsupported".to_owned())
                }
            })();
            Self::set(
                &state,
                match result {
                    Ok(()) => State::Running,
                    Err(e) => State::Failed(e),
                },
            );
            repaint();
        });
    }

    /// Shuts the engine down (waits up to a few seconds).
    pub fn stop(&self) {
        #[cfg(engine)]
        if let Some(engine) = self.running.lock().ok().and_then(|mut slot| slot.take()) {
            engine.stop();
        }
        let installed = self.paths.as_ref().is_some_and(Paths::installed);
        Self::set(
            &self.state,
            if installed {
                State::Stopped
            } else {
                State::NotInstalled
            },
        );
    }

    /// Whether the VM still runs (it may have stopped by itself).
    pub fn check(&self) {
        #[cfg(engine)]
        if self.state() == State::Running {
            let alive = self
                .running
                .lock()
                .ok()
                .and_then(|slot| slot.as_ref().map(platform::Running::is_running))
                .unwrap_or(false);
            if !alive {
                Self::set(&self.state, State::Failed("The engine stopped".into()));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(unix)]
    fn keeps_everything_in_one_folder() {
        let paths = Paths::under(PathBuf::from("/Users/me/.ferrodock"));
        assert_eq!(paths.host(), "unix:///Users/me/.ferrodock/docker.sock");
        assert_eq!(
            paths.kernel,
            PathBuf::from("/Users/me/.ferrodock/engine/kernel")
        );
        assert!(!paths.installed());
        let r = Resources::default();
        assert!((2..=8).contains(&r.cpus));
    }

    #[test]
    #[cfg(windows)]
    fn offers_the_engine_on_a_named_pipe() {
        let paths = Paths::under(PathBuf::from(r"C:\Users\me\AppData\Local\Ferrodock"));
        assert_eq!(paths.host(), PIPE);
        assert!(paths.distro.ends_with("wsl"));
        assert!(!paths.installed());
    }
}
