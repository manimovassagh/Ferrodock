//! Starting the Docker engine when it isn't running: the app or service
//! that provides it on this machine, found without asking the user.
//!
//! On macOS the engine runs in a VM some app manages. That app is found
//! from the engine itself: while connected, [`provider_app`] asks who holds
//! its socket, so the same app starts next time; before that, an installed
//! app named like the engine's context or socket folder is taken.

use std::path::{Path, PathBuf};
use std::process::Command;

/// A way to start the engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Starter {
    /// A macOS app (its `.app` folder), opened in the background.
    App(PathBuf),
    /// `colima start` for a Colima profile.
    Colima { binary: PathBuf, profile: String },
    /// A program to launch (the engine's app on Windows).
    Program(PathBuf),
    /// The `docker` system service (Linux).
    Service,
    /// The engine app's user service (Linux).
    UserService,
}

impl Starter {
    /// What it starts, e.g. "Colima".
    pub fn name(&self) -> String {
        match self {
            Starter::App(app) => app
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "Docker".into()),
            Starter::Colima { profile, .. } if profile == "default" => "Colima".into(),
            Starter::Colima { profile, .. } => format!("Colima ({profile})"),
            Starter::Program(_) | Starter::UserService => "the Docker engine".into(),
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

/// What an app or folder name boils down to for matching:
/// "Some Engine.app" and ".some-engine" both give "someengine".
fn normalized(name: &str) -> String {
    name.trim_start_matches('.')
        .trim_end_matches(".app")
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Names the engine at `host` goes by: its Docker context's name, and the
/// hidden folder its socket lives in (also behind a symlink), such as
/// `~/.engine/run/docker.sock` → "engine".
fn hints(host: &str, engine: &str) -> Vec<String> {
    let mut hints = vec![normalized(engine)];
    if let Some(socket) = socket_path(host) {
        let target = std::fs::read_link(&socket).ok();
        for path in std::iter::once(socket).chain(target) {
            let hidden = path
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .find(|c| c.starts_with('.') && c.len() > 1);
            if let Some(dir) = hidden {
                hints.push(normalized(&dir));
            }
        }
    }
    hints.retain(|h| !h.is_empty() && h != "default");
    hints
}

/// An installed macOS app whose name matches one of `hints`.
fn app_named(hints: &[String]) -> Option<PathBuf> {
    let home_apps = std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Applications"));
    let dirs = std::iter::once(PathBuf::from("/Applications")).chain(home_apps);
    dirs.filter_map(|dir| std::fs::read_dir(dir).ok())
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|e| e == "app"))
        .find(|path| {
            path.file_name()
                .is_some_and(|n| hints.contains(&normalized(&n.to_string_lossy())))
        })
}

/// How to start the engine at `host` (named `engine`, e.g. its Docker
/// context) on this machine. `remembered` is the app found by
/// [`provider_app`] while it was last running.
pub fn starter(host: &str, engine: &str, remembered: Option<&str>) -> Option<Starter> {
    if let Some(profile) = colima_profile(host) {
        return find_binary("colima").map(|binary| Starter::Colima { binary, profile });
    }
    if cfg!(target_os = "macos") {
        let remembered = remembered.map(PathBuf::from).filter(|app| app.is_dir());
        if let Some(app) = remembered.or_else(|| app_named(&hints(host, engine))) {
            return Some(Starter::App(app));
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
    // Linux: the engine app's user service, or the system service.
    let user_app = std::env::var_os("HOME")
        .map(|h| PathBuf::from(h).join(".docker/desktop"))
        .is_some_and(|p| p.is_dir());
    if user_app && host.contains(".docker/desktop") {
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
        let app = Starter::App(PathBuf::from("/Applications/Some Engine.app"));
        assert_eq!(app.name(), "Some Engine");
        assert_eq!(normalized("Some Engine.app"), normalized(".some-engine"));
        assert_eq!(
            hints("unix:///Users/me/.someengine/run/docker.sock", "work"),
            ["work", "someengine"]
        );
        assert!(hints("unix:///var/run/nothing-here.sock", "default").is_empty());
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
