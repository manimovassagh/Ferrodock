//! Booting the VM as its first process: kernel filesystems, the data
//! disk, shared folders and the network.

mod disk;
mod mounts;
mod network;
mod rosetta;

use crate::config::Config;
use crate::log;

/// Where programs are found, set for everything the agent starts.
pub const PATH: &str = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";

/// Gets the system ready for the engine. A step that fails is logged and
/// the others still run, so the engine comes up with as much as works.
pub fn boot(config: &Config) {
    log("booting");
    step(
        "hostname",
        std::fs::write("/proc/sys/kernel/hostname", &config.hostname).map_err(|e| e.to_string()),
    );
    step("data disk", disk::mount_data(&config.data_disk));
    for (tag, path) in &config.shares {
        step(&format!("share {tag}"), mounts::share(tag, path));
    }
    // Only when the host shares Rosetta (Apple Silicon, macOS 13+).
    if let Some(tag) = &config.rosetta {
        step("rosetta", rosetta::enable(tag));
    }
    step("network", network::up());
    log("booted");
}

/// Logs a step that failed.
fn step(name: &str, result: Result<(), String>) {
    if let Err(e) = result {
        log(&format!("{name}: {e}"));
    }
}

/// proc, sys, dev and scratch space: needed before anything else, even
/// reading the settings.
pub fn mount_kernel_filesystems() {
    step("filesystems", mounts::kernel_filesystems());
}

/// Runs a program to completion; its output goes to the console.
fn run(program: &str, args: &[&str]) -> Result<(), String> {
    let status = std::process::Command::new(program)
        .args(args)
        .status()
        .map_err(|e| format!("{program}: {e}"))?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| format!("{program} {}: {status}", args.join(" ")))
}
