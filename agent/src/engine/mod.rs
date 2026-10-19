//! Keeping the Docker engine running.

use std::process::Command;
use std::time::{Duration, Instant};

use crate::config::Config;
use crate::log;

/// Where the engine listens inside the VM.
pub const SOCKET: &str = "/var/run/docker.sock";

/// Restarts come no faster than this, so a broken engine doesn't spin.
const BACKOFF: Duration = Duration::from_secs(2);

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
