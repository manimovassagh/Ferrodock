//! Where Ferrodock's own Docker engine lives on this machine, and how to
//! reach it. Plain data shared by the app, which runs the engine, and by
//! everything else that only connects to it (the CLI, the MCP server).

use std::path::PathBuf;

/// How the engine shows up in the engine list.
pub const NAME: &str = "Ferrodock";
/// Linux: where the engine's programs are installed, its service and its
/// socket.
pub const LINUX_PROGRAMS: &str = "/opt/ferrodock/engine/bin";
pub const LINUX_SERVICE: &str = "/etc/systemd/system/ferrodock-engine.service";
pub const LINUX_SOCKET: &str = "/run/ferrodock/docker.sock";

/// Whether this machine can run the engine: Apple Silicon Macs, Windows
/// (x86_64) and Linux (x86_64 and arm64).
pub const SUPPORTED: bool = cfg!(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(windows, target_arch = "x86_64"),
    all(
        target_os = "linux",
        any(target_arch = "x86_64", target_arch = "aarch64")
    )
));

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

    /// Where the engine lives for this user, if there is a home folder.
    pub fn locate() -> Option<Self> {
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
        if cfg!(target_os = "linux") {
            return std::path::Path::new(LINUX_SERVICE).is_file()
                && std::path::Path::new(LINUX_PROGRAMS)
                    .join("dockerd")
                    .is_file();
        }
        self.kernel.is_file() && self.rootfs.is_file()
    }

    /// The Docker host clients connect to.
    pub fn host(&self) -> String {
        if cfg!(windows) {
            return PIPE.into();
        }
        if cfg!(target_os = "linux") {
            return format!("unix://{LINUX_SOCKET}");
        }
        format!("unix://{}", self.socket.display())
    }
}

/// The named pipe Ferrodock offers the engine on, on Windows.
pub const PIPE: &str = "npipe:////./pipe/ferrodock_engine";
