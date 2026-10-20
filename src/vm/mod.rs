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
//! - On Linux, where Docker runs natively, it's the Docker engine as a
//!   system service, `ferrodock-engine`, set up once with the user's
//!   admin password. Its socket is `/run/ferrodock/docker.sock`.
//!
//! The image (built in this repository's `engine/`) is downloaded on
//! first use.

#[cfg(any(engine, test))]
pub mod download;
#[cfg(all(engine, target_os = "linux"))]
mod linux;
#[cfg(all(engine, target_os = "linux"))]
use linux as platform;
#[cfg(all(engine, target_os = "macos"))]
mod macos;
#[cfg(all(engine, target_os = "macos"))]
use macos as platform;
#[cfg(any(all(engine, windows), test))]
mod mux;
#[cfg(any(all(engine, target_os = "macos"), test))]
mod ports;
#[cfg(all(engine, windows))]
mod windows;
#[cfg(any(all(engine, windows), test))]
mod wsl;
#[cfg(all(engine, windows))]
use windows as platform;

use std::sync::{Arc, Mutex};

pub use ferrodock_core::builtin::{NAME, Paths, SUPPORTED};
/// The virtiofs tag the home folder is shared under.
#[cfg(all(engine, target_os = "macos"))]
pub const SHARE_TAG: &str = "home";
/// Where the agent in the VM offers the engine's socket.
#[cfg(all(engine, target_os = "macos"))]
pub const VSOCK_PORT: u32 = 2375;

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

impl Resources {
    /// The most the VM can be given: every CPU, and the Mac's memory.
    pub fn most() -> Self {
        let cpus = std::thread::available_parallelism().map_or(4, |n| n.get() as u32);
        Self {
            cpus,
            memory_mib: physical_memory_mib().unwrap_or(16 * 1024),
        }
    }
}

/// The computer's memory, in MiB.
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn physical_memory_mib() -> Option<u64> {
    let mut bytes: u64 = 0;
    let mut size = std::mem::size_of::<u64>();
    // SAFETY: `hw.memsize` is a u64, written into `bytes`.
    let status = unsafe {
        libc::sysctlbyname(
            c"hw.memsize".as_ptr(),
            (&raw mut bytes).cast(),
            &raw mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    (status == 0).then_some(bytes / (1024 * 1024))
}

#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
fn physical_memory_mib() -> Option<u64> {
    None
}

/// Where the engine is at.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(not(engine), allow(dead_code))]
pub enum State {
    NotInstalled,
    /// Installing WSL, which the engine runs in (Windows).
    #[cfg_attr(not(windows), allow(dead_code))]
    InstallingWsl,
    /// WSL was installed and works once Windows restarts (Windows).
    #[cfg_attr(not(windows), allow(dead_code))]
    NeedsRestart,
    Downloading {
        done: u64,
        total: Option<u64>,
    },
    Stopped,
    Starting,
    Running,
    Failed(String),
}

impl State {
    pub fn busy(&self) -> bool {
        matches!(
            self,
            State::InstallingWsl | State::Downloading { .. } | State::Starting
        )
    }
}

/// The engine, driven from the UI. Long work (downloading, booting) runs on
/// its own thread; `state()` says how it goes.
pub struct Engine {
    paths: Option<Paths>,
    state: Arc<Mutex<State>>,
    /// What the running engine was given.
    given: Arc<Mutex<Option<Resources>>>,
    #[cfg(engine)]
    running: Arc<Mutex<Option<platform::Running>>>,
}

impl Engine {
    pub fn new() -> Self {
        let paths = Paths::locate();
        let state = match &paths {
            Some(p) if p.installed() => State::Stopped,
            _ => State::NotInstalled,
        };
        Self {
            paths,
            state: Arc::new(Mutex::new(state)),
            given: Arc::default(),
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
        if self.state().busy() || self.state() == State::Running {
            return;
        }
        let Some(boot) = self.boot(resources, repaint) else {
            return;
        };
        std::thread::spawn(boot);
    }

    /// Stops the engine and boots it again, with `resources`. Only where
    /// Ferrodock runs the engine itself (not the Linux service).
    pub fn restart(&self, resources: Resources, repaint: impl Fn() + Send + 'static) {
        if self.state().busy() || !Self::RESTARTS {
            return;
        }
        let Some(boot) = self.boot(resources, repaint) else {
            return;
        };
        #[cfg(engine)]
        let engine = self.running.lock().ok().and_then(|mut slot| slot.take());
        let state = self.state.clone();
        Self::set(&state, State::Starting);
        std::thread::spawn(move || {
            #[cfg(engine)]
            if let Some(engine) = engine {
                engine.stop();
            }
            boot();
        });
    }

    /// Whether [`Engine::restart`] does anything: on Linux the engine is a
    /// system service that keeps running on its own.
    pub const RESTARTS: bool = cfg!(any(target_os = "macos", windows));

    /// The work of starting: download, install, boot. None when it can't
    /// be done here (the reason is in the state).
    fn boot(
        &self,
        resources: Resources,
        repaint: impl Fn() + Send + 'static,
    ) -> Option<impl FnOnce() + Send + 'static> {
        let Some(paths) = self.paths.clone() else {
            Self::set(&self.state, State::Failed("No home folder".into()));
            return None;
        };
        if !SUPPORTED {
            Self::set(
                &self.state,
                State::Failed(
                    "Ferrodock's engine needs a Mac with Apple Silicon, Windows or Linux".into(),
                ),
            );
            return None;
        }
        let state = self.state.clone();
        let given = self.given.clone();
        #[cfg(engine)]
        let running = self.running.clone();
        Some(move || {
            let result = (|| {
                std::fs::create_dir_all(&paths.dir).map_err(|e| e.to_string())?;
                #[cfg(engine)]
                {
                    if !paths.installed() {
                        #[cfg(windows)]
                        if !platform::prepare(|| {
                            Self::set(&state, State::InstallingWsl);
                            repaint();
                        })? {
                            return Ok(State::NeedsRestart);
                        }
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
                    Ok::<State, String>(State::Running)
                }
                #[cfg(not(engine))]
                {
                    let _ = resources;
                    Err("unsupported".to_owned())
                }
            })();
            if let Ok(mut slot) = given.lock() {
                *slot = (result == Ok(State::Running)).then_some(resources);
            }
            Self::set(&state, result.unwrap_or_else(State::Failed));
            repaint();
        })
    }

    /// What the running engine was given, once it runs.
    pub fn resources(&self) -> Option<Resources> {
        self.given.lock().ok().and_then(|g| *g)
    }

    /// The engine's log file, where there is one (on Linux it's in the
    /// system journal: `journalctl -u ferrodock-engine`).
    pub fn log(&self) -> Option<std::path::PathBuf> {
        let console = &self.paths.as_ref()?.console;
        (Self::RESTARTS && console.is_file()).then(|| console.clone())
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
    use std::path::PathBuf;

    #[test]
    #[cfg(unix)]
    fn keeps_everything_in_one_folder() {
        let paths = Paths::under(PathBuf::from("/Users/me/.ferrodock"));
        let host = if cfg!(target_os = "linux") {
            "unix:///run/ferrodock/docker.sock"
        } else {
            "unix:///Users/me/.ferrodock/docker.sock"
        };
        assert_eq!(paths.host(), host);
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
        assert_eq!(paths.host(), ferrodock_core::builtin::PIPE);
        assert!(paths.distro.ends_with("wsl"));
        assert!(!paths.installed());
    }
}
