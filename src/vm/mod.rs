//! Ferrodock's own Docker engine: a small Linux system with the Docker
//! engine inside, so nothing else needs to be installed (#114).
//!
//! - On Macs (Apple Silicon and Intel) it's a VM, through Apple's
//!   Virtualization framework. Everything lives in `~/.ferrodock`, and
//!   the engine's socket is `~/.ferrodock/docker.sock`.
//! - On Windows it's a WSL distro called `ferrodock`, kept in
//!   `%LOCALAPPDATA%\Ferrodock`. Ferrodock carries the engine's socket
//!   over the distro's standard input and output, and offers it as the
//!   named pipe `\\.\pipe\ferrodock_engine`.
//! - On Linux, where Docker runs natively, it's the Docker engine as a
//!   system service, `ferrodock-engine`, set up once with the user's
//!   admin password. Its socket is `/run/ferrodock/docker.sock`.
//!
//! The image (built in this repository's `engine/`) is downloaded on
//! first use, and again when a newer Ferrodock brings a newer one; the
//! engine's images, containers and volumes stay. The engine is also
//! offered to the `docker` command as the `ferrodock` context.

#[cfg(any(engine, test))]
mod context;
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

pub use ferrodock_core::builtin::{CONTEXT, NAME, Paths, SUPPORTED};
/// The virtiofs tag the home folder is shared under.
#[cfg(all(engine, target_os = "macos"))]
pub const SHARE_TAG: &str = "home";
/// The virtiofs tag Rosetta is shared under (Apple Silicon).
#[cfg(all(engine, target_os = "macos"))]
pub const ROSETTA_TAG: &str = "rosetta";
/// Where the agent in the VM offers the engine's socket.
#[cfg(all(engine, target_os = "macos"))]
pub const VSOCK_PORT: u32 = 2375;

/// The sizes the VM's data disk can be, in GB.
pub const DISK_GIB: [u64; 4] = [32, 64, 128, 256];

/// What the VM gets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Resources {
    pub cpus: u32,
    pub memory_mib: u64,
    /// The data disk. It's sparse, so it only takes what's used, and it
    /// only ever grows: a smaller size needs a reset.
    pub disk_gib: u64,
}

impl Default for Resources {
    /// Half the CPUs (2 to 8), 4 GB and a 128 GB disk: enough for typical
    /// development stacks; unused memory goes back to macOS.
    fn default() -> Self {
        let cpus = std::thread::available_parallelism().map_or(4, |n| n.get() as u32);
        Self {
            cpus: (cpus / 2).clamp(2, 8),
            memory_mib: 4096,
            disk_gib: 128,
        }
    }
}

impl Resources {
    /// The most the VM can be given: every CPU, the Mac's memory and the
    /// largest disk.
    pub fn most() -> Self {
        let cpus = std::thread::available_parallelism().map_or(4, |n| n.get() as u32);
        Self {
            cpus,
            memory_mib: physical_memory_mib().unwrap_or(16 * 1024),
            disk_gib: DISK_GIB[DISK_GIB.len() - 1],
        }
    }
}

/// The computer's memory, in MiB.
#[cfg(all(engine, target_os = "macos"))]
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

#[cfg(not(all(engine, target_os = "macos")))]
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
    /// `update`: a newer image, for an engine that's set up already.
    Downloading {
        done: u64,
        total: Option<u64>,
        update: bool,
    },
    Stopped,
    Starting,
    Running,
    /// Shutting down, or for a reset, deleting its data too.
    Stopping,
    Failed(String),
}

impl State {
    pub fn busy(&self) -> bool {
        matches!(
            self,
            State::InstallingWsl | State::Downloading { .. } | State::Starting | State::Stopping
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
            Some(p) if p.installed() => {
                // Set up by a Ferrodock from before the context, or still
                // running as a service (Linux).
                #[cfg(engine)]
                let _ = context::add(&p.host());
                State::Stopped
            }
            _ => State::NotInstalled,
        };
        let engine = Self {
            paths,
            state: Arc::new(Mutex::new(state)),
            given: Arc::default(),
            #[cfg(engine)]
            running: Arc::default(),
        };
        // On Linux the service starts with the computer: it may run already.
        #[cfg(all(engine, target_os = "linux"))]
        if engine.state() == State::Stopped
            && let Some(found) = platform::Running::find()
        {
            Self::set(&engine.state, State::Running);
            if let Ok(mut slot) = engine.running.lock() {
                *slot = Some(found);
            }
        }
        engine
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

    /// Moves to `next` if the engine is in a state `from` accepts, checked
    /// and changed under one lock: a second click (or auto-start racing a
    /// click) then finds the engine busy instead of booting it twice.
    fn claim(&self, from: impl Fn(&State) -> bool, next: State) -> bool {
        let Ok(mut s) = self.state.lock() else {
            return false;
        };
        if !from(&s) {
            return false;
        }
        *s = next;
        true
    }

    /// Shows the engine as `state`, for screenshots.
    #[cfg(test)]
    pub fn pretend(&self, state: State) {
        Self::set(&self.state, state);
    }

    /// Downloads the image if needed, then boots the engine. `repaint`
    /// wakes the UI as things change.
    pub fn start(&self, resources: Resources, repaint: impl Fn() + Send + 'static) {
        if !self.claim(|s| !s.busy() && *s != State::Running, State::Starting) {
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
        if !Self::RESTARTS || !self.claim(|s| !s.busy(), State::Starting) {
            return;
        }
        let Some(boot) = self.boot(resources, repaint) else {
            return;
        };
        #[cfg(engine)]
        let engine = self.running.lock().ok().and_then(|mut slot| slot.take());
        std::thread::spawn(move || {
            #[cfg(engine)]
            if let Some(engine) = engine {
                engine.stop();
            }
            boot();
        });
    }

    /// Stops the engine when the user asks: on Linux too, where it's a
    /// service that otherwise keeps running.
    pub fn shut_down(&self, repaint: impl Fn() + Send + 'static) {
        if !self.claim(|s| *s == State::Running, State::Stopping) {
            return;
        }
        #[cfg(engine)]
        let engine = self.running.lock().ok().and_then(|mut slot| slot.take());
        let state = self.state.clone();
        let given = self.given.clone();
        std::thread::spawn(move || {
            let result = Ok(());
            #[cfg(engine)]
            let result = engine.map_or(result, |engine| engine.shut_down());
            if let Ok(mut slot) = given.lock() {
                *slot = None;
            }
            Self::set(
                &state,
                match result {
                    Ok(()) => State::Stopped,
                    Err(e) => State::Failed(e),
                },
            );
            repaint();
        });
    }

    /// Stops the engine, deletes everything in it (images, containers,
    /// volumes) and starts it again, empty, with `resources`.
    pub fn reset(&self, resources: Resources, repaint: impl Fn() + Send + Sync + 'static) {
        if !self.claim(|s| !s.busy(), State::Stopping) {
            return;
        }
        let repaint = Arc::new(repaint);
        let wake = repaint.clone();
        let Some(boot) = self.boot(resources, move || wake()) else {
            return;
        };
        let paths = self.paths.clone();
        #[cfg(engine)]
        let engine = self.running.lock().ok().and_then(|mut slot| slot.take());
        let state = self.state.clone();
        std::thread::spawn(move || {
            #[cfg(engine)]
            {
                if let Some(engine) = engine {
                    engine.stop();
                }
                if let Some(paths) = &paths
                    && let Err(e) = platform::reset(paths)
                {
                    Self::set(&state, State::Failed(e));
                    repaint();
                    return;
                }
            }
            #[cfg(not(engine))]
            let _ = (paths, repaint);
            boot();
        });
    }

    /// How big the data disk is now, in GB (macOS), once there is one.
    pub fn disk_gib(&self) -> Option<u64> {
        let data = &self.paths.as_ref()?.data;
        std::fs::metadata(data)
            .ok()
            .map(|m| m.len() / (1024 * 1024 * 1024))
    }

    /// Whether [`Engine::restart`] does anything: on Linux the engine is a
    /// system service that keeps running on its own.
    pub const RESTARTS: bool = cfg!(any(target_os = "macos", windows));

    /// The work of starting: download, install (or update), boot. None
    /// when it can't be done here (the reason is in the state).
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
                    "Ferrodock's engine needs a Mac, Windows (x86_64) or Linux (x86_64 or arm64)"
                        .into(),
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
                    let fetch = |update| {
                        download::fetch(|done, total| {
                            Self::set(
                                &state,
                                State::Downloading {
                                    done,
                                    total,
                                    update,
                                },
                            );
                            repaint();
                        })
                    };
                    if !paths.installed() {
                        #[cfg(windows)]
                        if !platform::prepare(|| {
                            Self::set(&state, State::InstallingWsl);
                            repaint();
                        })? {
                            return Ok(State::NeedsRestart);
                        }
                        let archive = fetch(false)?;
                        Self::set(&state, State::Starting);
                        repaint();
                        platform::install(&paths, &archive)?;
                        download::record(&paths)?;
                    }
                    // This Ferrodock brings a newer image. When it can't be
                    // had (offline) or put in, the one set up starts, and
                    // the update is tried again next time.
                    let mut updated = Ok(());
                    if download::outdated(&paths)
                        && let Ok(archive) = fetch(true)
                    {
                        Self::set(&state, State::Starting);
                        repaint();
                        updated = platform::update(&paths, &archive)
                            .and_then(|()| download::record(&paths));
                    }
                    Self::set(&state, State::Starting);
                    repaint();
                    let engine = platform::Running::start(&paths, &resources).map_err(|e| {
                        updated
                            .err()
                            .map_or(e, |u| format!("Updating the engine failed: {u}"))
                    })?;
                    if let Ok(mut slot) = running.lock() {
                        *slot = Some(engine);
                    }
                    // For `docker context use ferrodock`; not having it
                    // doesn't stop the engine.
                    let _ = context::add(&paths.host());
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
    fn a_second_start_finds_the_engine_busy() {
        let engine = Engine::new();
        engine.pretend(State::Stopped);
        let ready = |s: &State| !s.busy() && *s != State::Running;
        assert!(engine.claim(ready, State::Starting));
        assert!(!engine.claim(ready, State::Starting));
        assert_eq!(engine.state(), State::Starting);
        // Nor can it be stopped or reset halfway through.
        assert!(!engine.claim(|s| *s == State::Running, State::Stopping));
        assert!(!engine.claim(|s| !s.busy(), State::Stopping));
    }

    /// An engine in `state` with no home folder: nothing reaches the
    /// platform, so the transitions run on any OS.
    fn bare(state: State) -> Engine {
        Engine {
            paths: None,
            state: Arc::new(Mutex::new(state)),
            given: Arc::default(),
            #[cfg(engine)]
            running: Arc::default(),
        }
    }

    const BUSY: [State; 4] = [
        State::InstallingWsl,
        State::Downloading {
            done: 1,
            total: None,
            update: false,
        },
        State::Starting,
        State::Stopping,
    ];

    #[test]
    fn only_one_of_many_racing_starts_wins() {
        let engine = Arc::new(bare(State::Stopped));
        let ready = |s: &State| !s.busy() && *s != State::Running;
        let barrier = Arc::new(std::sync::Barrier::new(16));
        let wins = (0..16)
            .map(|_| {
                let (engine, barrier) = (engine.clone(), barrier.clone());
                std::thread::spawn(move || {
                    barrier.wait();
                    engine.claim(ready, State::Starting)
                })
            })
            .collect::<Vec<_>>()
            .into_iter()
            .map(|t| t.join().unwrap())
            .filter(|won| *won)
            .count();
        assert_eq!(wins, 1);
        assert_eq!(engine.state(), State::Starting);
    }

    #[test]
    fn leaves_a_busy_engine_alone() {
        for busy in BUSY {
            let engine = bare(busy.clone());
            engine.start(Resources::default(), || {});
            engine.restart(Resources::default(), || {});
            engine.reset(Resources::default(), || {});
            engine.shut_down(|| {});
            assert_eq!(engine.state(), busy);
        }
    }

    #[test]
    fn starts_only_what_isnt_running() {
        let engine = bare(State::Running);
        engine.start(Resources::default(), || {});
        assert_eq!(engine.state(), State::Running);
        // From stopped or failed it tries, and says why it can't here.
        for from in [
            State::Stopped,
            State::NotInstalled,
            State::Failed("x".into()),
        ] {
            let engine = bare(from);
            engine.start(Resources::default(), || {});
            assert_eq!(engine.state(), State::Failed("No home folder".into()));
        }
    }

    #[test]
    fn shuts_down_only_a_running_engine() {
        for from in [
            State::Stopped,
            State::NotInstalled,
            State::Failed("x".into()),
        ] {
            let engine = bare(from.clone());
            engine.shut_down(|| {});
            assert_eq!(engine.state(), from);
        }
        let engine = bare(State::Running);
        let (done, wait) = std::sync::mpsc::channel();
        engine.shut_down(move || {
            let _ = done.send(());
        });
        wait.recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        assert_eq!(engine.state(), State::Stopped);
        assert_eq!(engine.resources(), None);
    }

    #[test]
    fn stopping_settles_the_state() {
        let engine = bare(State::Running);
        engine.stop();
        // Nothing is set up without a home folder.
        assert_eq!(engine.state(), State::NotInstalled);
    }

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
