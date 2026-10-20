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
    /// The `docker` system service (Linux).
    Service,
}

impl Starter {
    /// What it starts, for messages.
    /// Never another product's name: Ferrodock speaks of the engine.
    pub fn name(&self) -> String {
        match self {
            Starter::Service => "the Docker service".into(),
            _ => "the Docker engine".into(),
        }
    }

    /// Starts it. Returns once it's asked to start; the engine may take a
    /// while longer to answer.
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
            Starter::Service => status(Command::new("systemctl").args(["start", "docker"]))
                // Without permission, ask for it the graphical way.
                .or_else(|_| status(Command::new("pkexec").args(["systemctl", "start", "docker"]))),
        };
        result.map_err(|e| format!("Couldn't start {}: {e}", self.name()))
    }
}

/// The socket file of a `unix://` host.
fn socket_path(host: &str) -> Option<PathBuf> {
    host.strip_prefix("unix://").map(PathBuf::from)
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
    if cfg!(target_os = "macos") {
        let remembered = remembered.map(PathBuf::from).filter(|app| app.is_dir());
        return remembered
            .or_else(|| app_named(&hints(host, engine)))
            .map(Starter::App);
    }
    if cfg!(windows) {
        return None;
    }
    // Linux: the system service.
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
    fn finds_the_app() {
        assert_eq!(
            outermost_app(Path::new(
                "/Applications/Some Engine.app/Contents/Frameworks/Helper.app/Contents/MacOS/helper"
            )),
            Some(Path::new("/Applications/Some Engine.app"))
        );
        assert_eq!(outermost_app(Path::new("/usr/bin/dockerd")), None);
        let app = Starter::App(PathBuf::from("/Applications/Some Engine.app"));
        assert_eq!(app.name(), "the Docker engine");
        assert_eq!(normalized("Some Engine.app"), normalized(".some-engine"));
        assert_eq!(
            hints("unix:///Users/me/.someengine/run/docker.sock", "work"),
            ["work", "someengine"]
        );
        assert!(hints("unix:///var/run/nothing-here.sock", "default").is_empty());
        assert_eq!(Starter::Service.name(), "the Docker service");
    }
}
