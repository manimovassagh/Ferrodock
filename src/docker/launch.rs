//! Starting the Docker engine when it isn't running: the app or service
//! that provides it on this machine, found without asking the user.
//!
//! On macOS the engine runs in a VM some app manages. While connected,
//! [`provider_app`] finds that app from who holds the engine's socket, so
//! Ferrodock can start the same one next time; otherwise it falls back to
//! the apps and tools it knows.

use std::path::{Path, PathBuf};
use std::process::Command;

/// A way to start the engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Starter {
    /// A macOS app (its `.app` folder), opened in the background.
    App(PathBuf),
    /// `colima start` for a Colima profile.
    Colima { binary: PathBuf, profile: String },
    /// A program to launch (Docker Desktop on Windows).
    Program(PathBuf),
    /// The `docker` system service (Linux).
    Service,
    /// Docker Desktop's user service (Linux).
    UserService,
}

impl Starter {
    /// What it starts, e.g. "Docker Desktop".
    pub fn name(&self) -> String {
        match self {
            Starter::App(app) => app
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "Docker".into()),
            Starter::Colima { profile, .. } if profile == "default" => "Colima".into(),
            Starter::Colima { profile, .. } => format!("Colima ({profile})"),
            Starter::Program(_) | Starter::UserService => "Docker Desktop".into(),
            Starter::Service => "the Docker service".into(),
        }
    }

    /// Starts it. Returns once it's asked to start; the engine may take a
    /// while longer to answer (Colima waits until its VM is up).
    pub fn run(&self) -> Result<(), String> {
        let status = |command: &mut Command| {
            command.status().map_err(|e| e.to_string()).and_then(|s| {
                s.success()
                    .then_some(())
                    .ok_or_else(|| format!("it exited with {s}"))
            })
        };
        let result = match self {
            // -g: don't bring it to the front; -j: launch it hidden.
            Starter::App(app) => status(Command::new("/usr/bin/open").arg("-gj").arg(app)),
            Starter::Colima { binary, profile } => {
                let mut command = Command::new(binary);
                command.args(["start", "--profile", profile]);
                // Colima calls Lima and docker, which live next to it.
                if let Some(dir) = binary.parent() {
                    command.env("PATH", with_path(dir));
                }
                status(&mut command)
            }
            Starter::Program(program) => Command::new(program)
                .spawn()
                .map(drop)
                .map_err(|e| e.to_string()),
            Starter::Service => status(Command::new("systemctl").args(["start", "docker"]))
                // Without permission, ask for it the graphical way.
                .or_else(|_| status(Command::new("pkexec").args(["systemctl", "start", "docker"]))),
            Starter::UserService => {
                status(Command::new("systemctl").args(["--user", "start", "docker-desktop"]))
            }
        };
        result.map_err(|e| format!("Couldn't start {}: {e}", self.name()))
    }
}

/// `PATH` with `dir` in front.
fn with_path(dir: &Path) -> std::ffi::OsString {
    let mut paths = vec![dir.to_path_buf()];
    if let Some(path) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&path));
    }
    std::env::join_paths(paths).unwrap_or_default()
}

/// The socket file of a `unix://` host.
fn socket_path(host: &str) -> Option<PathBuf> {
    host.strip_prefix("unix://").map(PathBuf::from)
}

/// The Colima profile whose socket `host` is, if it is one.
fn colima_profile(host: &str) -> Option<String> {
    let path = socket_path(host)?;
    let mut parts = path.components().rev().skip(1);
    let profile = parts.next()?.as_os_str().to_string_lossy().into_owned();
    let parent = parts.next()?.as_os_str().to_string_lossy().into_owned();
    (parent == ".colima").then_some(profile)
}

/// The first of `names` found in the usual install folders or on `PATH`.
/// Apps started from the Dock don't get the shell's `PATH`, so Homebrew's
/// folders are checked by hand.
fn find_binary(name: &str) -> Option<PathBuf> {
    let mut dirs = vec![
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/usr/bin"),
    ];
    if let Some(path) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&path));
    }
    dirs.into_iter().map(|d| d.join(name)).find(|p| p.is_file())
}

/// How to start the engine at `host` on this machine. `remembered` is the
/// app found by [`provider_app`] while it was last running.
pub fn starter(host: &str, remembered: Option<&str>) -> Option<Starter> {
    if let Some(profile) = colima_profile(host) {
        return find_binary("colima").map(|binary| Starter::Colima { binary, profile });
    }
    if cfg!(target_os = "macos") {
        let apps = remembered.into_iter().map(PathBuf::from).chain(
            ["Docker.app", "Rancher Desktop.app"]
                .into_iter()
                .map(|a| Path::new("/Applications").join(a)),
        );
        for app in apps {
            if app.is_dir() {
                return Some(Starter::App(app));
            }
        }
        return find_binary("colima").map(|binary| Starter::Colima {
            binary,
            profile: "default".into(),
        });
    }
    if cfg!(windows) {
        let base = std::env::var_os("ProgramFiles")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\Program Files"));
        let program = base.join(r"Docker\Docker\Docker Desktop.exe");
        return program.is_file().then_some(Starter::Program(program));
    }
    // Linux: Docker Desktop, or the system service.
    let desktop = std::env::var_os("HOME")
        .map(|h| PathBuf::from(h).join(".docker/desktop"))
        .is_some_and(|p| p.is_dir());
    if desktop && host.contains(".docker/desktop") {
        return Some(Starter::UserService);
    }
    let units = [
        "/etc/systemd/system/docker.service",
        "/lib/systemd/system/docker.service",
        "/usr/lib/systemd/system/docker.service",
    ];
    units
        .iter()
        .any(|u| Path::new(u).exists())
        .then_some(Starter::Service)
}

/// The macOS app serving the engine at `host`: the outermost `.app` of the
/// process holding its socket. None elsewhere, or if it can't be told.
pub fn provider_app(host: &str) -> Option<String> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    let socket = std::fs::canonicalize(socket_path(host)?).ok()?;
    let output = Command::new("/usr/sbin/lsof")
        .arg("-t")
        .arg(&socket)
        .output()
        .ok()?;
    let pid = String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()?
        .trim()
        .to_owned();
    let output = Command::new("/bin/ps")
        .args(["-o", "comm=", "-p", &pid])
        .output()
        .ok()?;
    let program = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    outermost_app(Path::new(&program)).map(|app| app.display().to_string())
}

/// `/Applications/X.app/Contents/Frameworks/Y.app/…` → `/Applications/X.app`.
fn outermost_app(path: &Path) -> Option<&Path> {
    path.ancestors()
        .filter(|p| p.extension().is_some_and(|e| e == "app"))
        .last()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_app_and_colima_profile() {
        assert_eq!(
            outermost_app(Path::new(
                "/Applications/Some Engine.app/Contents/Frameworks/Helper.app/Contents/MacOS/helper"
            )),
            Some(Path::new("/Applications/Some Engine.app"))
        );
        assert_eq!(outermost_app(Path::new("/usr/bin/dockerd")), None);
        assert_eq!(
            colima_profile("unix:///Users/me/.colima/work/docker.sock").as_deref(),
            Some("work")
        );
        assert_eq!(colima_profile("unix:///var/run/docker.sock"), None);
        let app = Starter::App(PathBuf::from("/Applications/Docker.app"));
        assert_eq!(app.name(), "Docker");
        assert_eq!(
            Starter::Colima {
                binary: PathBuf::new(),
                profile: "default".into()
            }
            .name(),
            "Colima"
        );
    }
}
