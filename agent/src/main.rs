//! `ferrodock-agent`: the first program in Ferrodock's Linux VM.
//!
//! It boots the system (mounts, network, the data disk, the folders the
//! host shares), keeps the Docker engine running, and carries the
//! engine's socket out to the host over vsock. Everything it needs to know
//! comes from the kernel command line, which the host sets.

#[cfg(target_os = "linux")]
mod bridge;
mod config;
#[cfg(target_os = "linux")]
mod engine;
#[cfg(target_os = "linux")]
mod system;

#[cfg(target_os = "linux")]
fn main() {
    let pid1 = std::process::id() == 1;
    if pid1 {
        // The kernel starts the first process with no PATH at all.
        // SAFETY: nothing else runs yet, so no other thread reads it.
        unsafe { std::env::set_var("PATH", system::PATH) };
        // /proc, where the settings come from, isn't mounted yet.
        system::mount_kernel_filesystems();
    }
    let cmdline = std::fs::read_to_string("/proc/cmdline").unwrap_or_default();
    let config = config::Config::parse(&cmdline);
    if pid1 {
        system::boot(&config);
    }
    bridge::serve(&config);
    // As PID 1 the agent also reaps every orphan, so it supervises the
    // engine from the same loop.
    engine::supervise(&config, pid1);
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("ferrodock-agent only runs inside Ferrodock's Linux VM");
    std::process::exit(1);
}

/// A line on the VM's console, which the host keeps as the engine log.
#[cfg(target_os = "linux")]
pub fn log(line: &str) {
    eprintln!("ferrodock-agent: {line}");
}
