//! Keeping the Docker engine running.

use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::time::{Duration, Instant};

use crate::config::Config;
use crate::log;

/// Where the engine listens inside the VM.
pub const SOCKET: &str = "/var/run/docker.sock";

/// Restarts come no faster than this, so a broken engine doesn't spin.
const BACKOFF: Duration = Duration::from_secs(2);
/// How long the engine gets to stop its containers when asked to stop.
const GRACE: Duration = Duration::from_secs(15);

/// The running engine's process, and whether it's being stopped for good.
static ENGINE: AtomicI32 = AtomicI32::new(0);
static STOPPING: AtomicBool = AtomicBool::new(false);

fn start() -> Option<u32> {
    let log_file = std::fs::File::create("/var/log/dockerd.log").ok();
    let mut command = Command::new("dockerd");
    command.args(["--host", &format!("unix://{SOCKET}")]);
    if let Some(file) = log_file
        && let Ok(copy) = file.try_clone()
    {
        command.stdout(copy).stderr(file);
    }
    match command.spawn() {
        Ok(child) => {
            log(&format!("engine started (pid {})", child.id()));
            ENGINE.store(child.id() as i32, Ordering::SeqCst);
            Some(child.id())
        }
        Err(e) => {
            log(&format!("engine: {e}"));
            None
        }
    }
}

/// Starts the engine and restarts it whenever it stops. As PID 1 it also
/// reaps every other process that ends. Never returns.
pub fn supervise(_config: &Config, pid1: bool) -> ! {
    let mut engine = start();
    let mut started = Instant::now();
    loop {
        let mut status = 0;
        // SAFETY: plain waitpid; any child of ours may be reaped.
        let pid = unsafe { libc::waitpid(-1, &mut status, 0) };
        if pid < 0 {
            // No children at all (not PID 1 and the engine never started).
            std::thread::sleep(BACKOFF);
        } else if Some(pid as u32) != engine {
            continue;
        } else {
            log(&format!("engine stopped (status {status})"));
            if STOPPING.load(Ordering::SeqCst) {
                std::process::exit(0);
            }
        }
        let since = started.elapsed();
        if since < BACKOFF {
            std::thread::sleep(BACKOFF - since);
        }
        let _ = std::fs::remove_file(SOCKET);
        engine = start();
        started = Instant::now();
        if !pid1 && engine.is_none() {
            std::process::exit(1);
        }
    }
}

/// Stops the engine for good, letting it shut its containers down, and
/// then the agent: [`supervise`] exits once the engine is gone.
pub fn stop() {
    STOPPING.store(true, Ordering::SeqCst);
    let pid = ENGINE.load(Ordering::SeqCst);
    if pid > 0 {
        log("stopping the engine");
        // SAFETY: plain kill on our own child.
        unsafe { libc::kill(pid, libc::SIGTERM) };
    }
    std::thread::sleep(GRACE);
    log("the engine didn't stop in time");
    std::process::exit(0);
}
