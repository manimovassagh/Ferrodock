//! `ferrodock-agent`: the first program in Ferrodock's Linux VM.
//!
//! It boots the system (mounts, network, the data disk, the folders the
//! host shares), keeps the Docker engine running, and carries the
//! engine's socket out to the host over vsock. Everything it needs to know
//! comes from the kernel command line, which the host sets.
//!
//! On Windows it runs in a WSL distro instead, started with `--stdio`:
//! WSL has already booted the system, so it only runs the engine and
//! carries its socket over standard input and output.

#[cfg(target_os = "linux")]
mod bridge;
#[cfg(target_os = "linux")]
mod certs;
mod config;
#[cfg(target_os = "linux")]
mod engine;
#[cfg(target_os = "linux")]
mod stdio;
#[cfg(target_os = "linux")]
mod system;

#[cfg(target_os = "linux")]
fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--stdio") {
        unsafe_set_path();
        // `--certs <file>`: what the host trusts, for the engine to trust.
        if let Some(file) = args.iter().skip_while(|a| *a != "--certs").nth(1)
            && let Err(e) = certs::add(std::path::Path::new(file))
        {
            log(&format!("certificates: {e}"));
        }
        std::thread::spawn(|| {
            let output = std::io::stdout();
            stdio::run(
                std::io::stdin().lock(),
                output,
                std::path::Path::new(engine::SOCKET),
            );
            // Ferrodock is gone or stopped the engine.
            engine::stop();
        });
        engine::supervise(&config::Config::default(), false);
    }
    let pid1 = std::process::id() == 1;
    if pid1 {
        // The kernel starts the first process with no PATH at all.
        unsafe_set_path();
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

/// Sets PATH for everything the agent starts. Only called first thing,
/// before any other thread runs.
#[cfg(target_os = "linux")]
fn unsafe_set_path() {
    // SAFETY: nothing else runs yet, so no other thread reads it.
    unsafe { std::env::set_var("PATH", system::PATH) };
}

/// A line on the VM's console, which the host keeps as the engine log.
#[cfg(target_os = "linux")]
pub fn log(line: &str) {
    eprintln!("ferrodock-agent: {line}");
}
