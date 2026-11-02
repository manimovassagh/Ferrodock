//! Local Podman engines, reached through their Docker-compatible API.
//! Machine configs provide names; Podman forwards each machine's API to
//! `<runtime>/podman/<name>-api.sock`, even without a Docker context.

use std::path::Path;

#[cfg(unix)]
use std::{os::unix::fs::FileTypeExt, path::PathBuf};

use crate::Engine;

/// Recognize Podman endpoints already registered as Docker contexts too.
pub(crate) fn is_engine(host: &str) -> bool {
    socket_name(host) == Some("podman.sock") || machine_name(host).is_some()
}

/// The configured machine name encoded in a forwarded Podman API socket.
pub(crate) fn machine_name(host: &str) -> Option<&str> {
    socket_name(host)?
        .strip_suffix("-api.sock")
        .filter(|name| valid_machine_name(name))
}

fn socket_name(host: &str) -> Option<&str> {
    let path = Path::new(host.strip_prefix("unix://")?);
    if path.parent()?.file_name()? != "podman" {
        return None;
    }
    path.file_name()?.to_str()
}

fn valid_machine_name(name: &str) -> bool {
    name.as_bytes()
        .first()
        .is_some_and(u8::is_ascii_alphanumeric)
        && name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_.-".contains(&c))
}

/// Discover local engines without invoking the Podman CLI, which may not
/// be on PATH when Ferrodock opens from the desktop.
pub(crate) fn engines() -> Vec<Engine> {
    #[cfg(unix)]
    {
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")));
        let runtime = if cfg!(target_os = "macos") {
            Some(std::env::temp_dir())
        } else {
            std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from)
        };
        discover(
            config.as_deref(),
            runtime.as_deref(),
            Path::new("/run/podman/podman.sock"),
        )
    }
    #[cfg(not(unix))]
    Vec::new()
}

#[cfg(unix)]
fn engine(name: String, socket: &Path) -> Engine {
    Engine {
        name,
        host: format!("unix://{}", socket.display()),
        tls: None,
        current: false,
    }
}

#[cfg(unix)]
fn is_socket(path: &Path) -> bool {
    path.metadata().is_ok_and(|m| m.file_type().is_socket())
}

#[cfg(unix)]
fn discover(config: Option<&Path>, runtime: Option<&Path>, rootful: &Path) -> Vec<Engine> {
    let mut found = Vec::new();
    if let Some(runtime) = runtime {
        let socket = runtime.join("podman/podman.sock");
        if is_socket(&socket) {
            found.push(engine("Podman".into(), &socket));
        }
        if let Some(config) = config {
            let machines = config.join("containers/podman/machine");
            // A provider (applehv, libkrun, qemu...) owns one directory.
            // Also check the root for configurations from older releases.
            found.extend(machine_engines(&machines, runtime));
            if let Ok(entries) = std::fs::read_dir(&machines) {
                for entry in entries.flatten() {
                    let dir = entry.path();
                    if dir.is_dir() {
                        found.extend(machine_engines(&dir, runtime));
                    }
                }
            }
        }
    }
    if is_socket(rootful) {
        found.push(engine("Podman (rootful)".into(), rootful));
    }
    found.sort_by(|a, b| a.name.cmp(&b.name).then(a.host.cmp(&b.host)));
    found.dedup_by(|a, b| a.host == b.host);
    found
}

#[cfg(unix)]
fn machine_engines(dir: &Path, runtime: &Path) -> Vec<Engine> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension()?.to_str()? != "json" {
                return None;
            }
            let text = std::fs::read_to_string(&path).ok()?;
            let config: serde_json::Value = serde_json::from_str(&text).ok()?;
            let name = config["Name"].as_str()?;
            // A name forms one filename, never a path. Ignore unrelated
            // JSON and unsupported configurations rather than inventing
            // endpoints for them.
            if !valid_machine_name(name)
                || path.file_stem()?.to_str()? != name
                || config["Version"].as_u64()? == 0
                || !config["SSH"].is_object()
            {
                return None;
            }
            let socket = runtime.join("podman").join(format!("{name}-api.sock"));
            // Keep configured machines visible while stopped so a saved
            // engine selection survives restarting the machine.
            Some(engine(format!("Podman ({name})"), &socket))
        })
        .collect()
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;

    #[test]
    fn recognizes_local_contexts_without_treating_remote_names_as_local() {
        assert!(is_engine("unix:///tmp/podman/dev-api.sock"));
        assert!(is_engine("unix:///run/podman/podman.sock"));
        assert!(!is_engine("unix:///var/run/docker.sock"));
        assert!(!is_engine("tcp://prod:2376"));
        assert!(!is_engine("ssh://prod"));
        assert_eq!(machine_name("unix:///tmp/podman/dev-api.sock"), Some("dev"));
        assert_eq!(machine_name("unix:///run/podman/podman.sock"), None);
        assert_eq!(machine_name("unix:///tmp/podman/-dev-api.sock"), None);
    }

    #[test]
    fn discovers_valid_machines_and_native_sockets() {
        // Keep Unix socket paths short enough for macOS.
        let dir = PathBuf::from("/tmp").join(format!("fd-podman-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let runtime = dir.join("runtime");
        let socket = runtime.join("podman/podman.sock");
        let rootful = dir.join("rootful.sock");
        std::fs::create_dir_all(socket.parent().unwrap()).unwrap();
        let discover = || super::discover(Some(&dir), Some(&runtime), &rootful);

        std::fs::write(&socket, "").unwrap();
        assert!(discover().is_empty(), "regular files aren't sockets");
        std::fs::remove_file(&socket).unwrap();
        let _rootless = UnixListener::bind(&socket).unwrap();
        let _rootful = UnixListener::bind(&rootful).unwrap();
        assert_eq!(discover().len(), 2);
        assert_eq!(
            super::discover(None, None, &rootful)[0].name,
            "Podman (rootful)"
        );

        for (provider, name, json) in [
            ("applehv", "z", r#"{"Name":"z","Version":1,"SSH":{}}"#),
            ("qemu", "a", r#"{"Name":"a","Version":1,"SSH":{}}"#),
            ("libkrun", "z", r#"{"Name":"z","Version":1,"SSH":{}}"#),
            ("", "legacy", r#"{"Name":"legacy","Version":1,"SSH":{}}"#),
            ("applehv", "broken", "{"),
            ("applehv", "other", r#"{"Name":"other"}"#),
            (
                "applehv",
                "bad",
                r#"{"Name":"../bad","Version":1,"SSH":{}}"#,
            ),
            (
                "applehv",
                "mismatch",
                r#"{"Name":"different","Version":1,"SSH":{}}"#,
            ),
            ("applehv", "old", r#"{"Name":"old","Version":0,"SSH":{}}"#),
        ] {
            let config = dir.join("containers/podman/machine").join(provider);
            std::fs::create_dir_all(&config).unwrap();
            std::fs::write(config.join(format!("{name}.json")), json).unwrap();
        }
        let found = discover();
        let names: Vec<_> = found.iter().map(|engine| engine.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "Podman",
                "Podman (a)",
                "Podman (legacy)",
                "Podman (rootful)",
                "Podman (z)"
            ]
        );
        // Configured machines remain discoverable without a running socket.
        assert_eq!(
            found[1].host,
            format!("unix://{}/podman/a-api.sock", runtime.display())
        );
        assert!(found[1].tls.is_none() && !found[1].current);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
