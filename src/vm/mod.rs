//! Ferrodock's own Docker engine: a small Linux VM with the Docker engine
//! inside, so nothing else needs to be installed (#114).
//!
//! It runs on Apple Silicon Macs, through Apple's Virtualization
//! framework. The image (kernel and root filesystem, built in this
//! repository's `engine/`) is downloaded on first use. Everything lives in
//! `~/.ferrodock`, and the engine's socket is `~/.ferrodock/docker.sock`.

pub mod download;
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod macos;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// How the engine shows up in the engine list.
pub const NAME: &str = "Ferrodock";
/// The virtiofs tag the home folder is shared under.
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub const SHARE_TAG: &str = "home";
/// Where the agent in the VM offers the engine's socket.
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub const VSOCK_PORT: u32 = 2375;
/// Size of the data disk (sparse: it only takes what's used).
const DATA_DISK: u64 = 128 * 1024 * 1024 * 1024;

/// Whether this machine can run the engine.
pub const SUPPORTED: bool = cfg!(all(target_os = "macos", target_arch = "aarch64"));

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
    /// The VM's console: the agent's and the kernel's messages.
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
            console: dir.join("console.log"),
            image,
            dir,
        }
    }

    pub fn default() -> Option<Self> {
        std::env::var_os("HOME").map(|home| Self::under(PathBuf::from(home).join(".ferrodock")))
    }

    /// Whether the image is downloaded.
    pub fn installed(&self) -> bool {
        self.kernel.is_file() && self.rootfs.is_file()
    }

    /// The Docker host clients connect to.
    pub fn host(&self) -> String {
        format!("unix://{}", self.socket.display())
    }
}

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
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    running: Arc<Mutex<Option<macos::Running>>>,
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
            #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
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
                State::Failed("Ferrodock's engine needs a Mac with Apple Silicon".into()),
            );
            return;
        }
        let state = self.state.clone();
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        let running = self.running.clone();
        std::thread::spawn(move || {
            let result = (|| {
                std::fs::create_dir_all(&paths.dir).map_err(|e| e.to_string())?;
                if !paths.installed() {
                    download::install(&paths.image, |done, total| {
                        Self::set(&state, State::Downloading { done, total });
                        repaint();
                    })?;
                }
                if !paths.data.exists() {
                    std::fs::File::create(&paths.data)
                        .and_then(|f| f.set_len(DATA_DISK))
                        .map_err(|e| format!("{}: {e}", paths.data.display()))?;
                }
                Self::set(&state, State::Starting);
                repaint();
                #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
                {
                    let engine = macos::Running::start(&paths, &resources)?;
                    if let Ok(mut slot) = running.lock() {
                        *slot = Some(engine);
                    }
                    Ok::<(), String>(())
                }
                #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
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
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
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
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        if self.state() == State::Running {
            let alive = self
                .running
                .lock()
                .ok()
                .and_then(|slot| slot.as_ref().map(macos::Running::is_running))
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
}
