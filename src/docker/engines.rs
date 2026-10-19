//! Docker engines to connect to: Ferrodock's own, the Docker CLI's
//! contexts, and a Docker socket found in the user's Docker folder (listed
//! as "Local engine").

use std::path::{Path, PathBuf};

use bollard::{API_DEFAULT_VERSION, Docker};
use serde_json::Value;

/// Seconds before a request to the engine gives up.
const TIMEOUT: u64 = 120;

/// One engine Ferrodock can talk to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Engine {
    /// The context's name, or "Local engine" for a detected socket.
    pub name: String,
    /// Where it listens: `unix://…`, `npipe://…`, `tcp://…` or `ssh://…`.
    pub host: String,
    /// Certificates for a TLS connection: (ca, cert, key).
    pub tls: Option<(PathBuf, PathBuf, PathBuf)>,
    /// The Docker CLI's current context.
    pub current: bool,
}

impl Engine {
    /// Engines this build can't connect to are listed but disabled.
    pub fn supported(&self) -> bool {
        let host = self.host.as_str();
        host.starts_with("unix://") && cfg!(unix)
            || host.starts_with("npipe://") && cfg!(windows)
            || host.starts_with("tcp://")
            || host.starts_with("http://")
            || host.starts_with("https://")
            || host.starts_with("ssh://") && cfg!(unix)
    }

    pub fn connect(&self) -> Result<Docker, String> {
        let error = |e: bollard::errors::Error| format!("Can't use {}: {e}", self.host);
        match &self.tls {
            Some((ca, cert, key)) => {
                Docker::connect_with_ssl(&self.host, key, cert, ca, TIMEOUT, API_DEFAULT_VERSION)
                    .map_err(error)
            }
            #[cfg(unix)]
            None if self.host.starts_with("ssh://") => {
                Docker::connect_with_ssh(&self.host, TIMEOUT, API_DEFAULT_VERSION, None)
                    .map_err(error)
            }
            None => Docker::connect_with_host(&self.host).map_err(error),
        }
    }
}

/// The Docker CLI's config folder.
fn docker_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("DOCKER_CONFIG") {
        return Some(PathBuf::from(dir));
    }
    home().map(|h| h.join(".docker"))
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

/// The engine used when nothing else is picked: `DOCKER_HOST`, or the
/// platform's standard socket.
fn default_host() -> String {
    std::env::var("DOCKER_HOST").unwrap_or_else(|_| {
        if cfg!(windows) {
            "npipe:////./pipe/docker_engine".into()
        } else {
            "unix:///var/run/docker.sock".into()
        }
    })
}

/// The contexts in the Docker CLI's config folder `dir`.
fn contexts(dir: &Path, current: &str) -> Vec<Engine> {
    let Ok(entries) = std::fs::read_dir(dir.join("contexts").join("meta")) else {
        return Vec::new();
    };
    let mut list: Vec<Engine> = entries
        .flatten()
        .filter_map(|entry| {
            let text = std::fs::read_to_string(entry.path().join("meta.json")).ok()?;
            let meta: Value = serde_json::from_str(&text).ok()?;
            let name = meta["Name"].as_str()?.to_owned();
            let endpoint = &meta["Endpoints"]["docker"];
            let host = endpoint["Host"].as_str()?.to_owned();
            // TLS files live under contexts/tls/<same hash>/docker/.
            let tls_dir = dir
                .join("contexts")
                .join("tls")
                .join(entry.file_name())
                .join("docker");
            let tls = [
                tls_dir.join("ca.pem"),
                tls_dir.join("cert.pem"),
                tls_dir.join("key.pem"),
            ];
            let tls = tls
                .iter()
                .all(|f| f.exists())
                .then(|| (tls[0].clone(), tls[1].clone(), tls[2].clone()));
            Some(Engine {
                current: name == current,
                name,
                host,
                tls,
            })
        })
        .collect();
    list.sort_by(|a, b| a.name.cmp(&b.name));
    list
}

/// The Docker socket in the user's own Docker folder, if there's one.
fn detected(home: &Path) -> Vec<Engine> {
    let socket = home.join(".docker/run/docker.sock");
    if !socket.exists() {
        return Vec::new();
    }
    vec![Engine {
        name: "Local engine".into(),
        host: format!("unix://{}", socket.display()),
        tls: None,
        current: false,
    }]
}

/// Every engine to offer: the default, the CLI's contexts, and detected
/// sockets no context already points at.
pub fn engines() -> Vec<Engine> {
    let dir = docker_dir();
    let config: Value = dir
        .as_ref()
        .and_then(|d| std::fs::read_to_string(d.join("config.json")).ok())
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or(Value::Null);
    let current = std::env::var("DOCKER_CONTEXT")
        .ok()
        .or_else(|| config["currentContext"].as_str().map(str::to_owned))
        .unwrap_or_else(|| "default".into());
    let mut list = vec![Engine {
        name: "default".into(),
        host: default_host(),
        tls: None,
        current: current == "default",
    }];
    if let Some(dir) = &dir {
        list.extend(contexts(dir, &current));
    }
    if let Some(home) = home() {
        for engine in detected(&home) {
            if list.iter().all(|e| e.host != engine.host) {
                list.push(engine);
            }
        }
    }
    // Ferrodock's own engine, once it's downloaded.
    if let Some(engine) = builtin_engine()
        && crate::vm::Paths::default().is_some_and(|p| p.installed())
    {
        list.push(engine);
    }
    list
}

/// Ferrodock's own engine (#114), where this machine can run it.
pub fn builtin_engine() -> Option<Engine> {
    if !crate::vm::SUPPORTED {
        return None;
    }
    let paths = crate::vm::Paths::default()?;
    Some(Engine {
        name: crate::vm::NAME.into(),
        host: paths.host(),
        tls: None,
        current: false,
    })
}

/// The engine called `name`, or the CLI's current one when there's no such
/// engine (or no name).
pub fn pick(list: &[Engine], name: Option<&str>) -> Option<Engine> {
    name.and_then(|n| list.iter().find(|e| e.name == n))
        .or_else(|| list.iter().find(|e| e.current))
        .or_else(|| list.first())
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ferrodock-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn reads_contexts() {
        let dir = temp("contexts");
        let meta = dir.join("contexts/meta/abc123");
        std::fs::create_dir_all(&meta).unwrap();
        std::fs::write(
            meta.join("meta.json"),
            r#"{"Name":"work","Endpoints":{"docker":{"Host":"unix:///x/docker.sock"}}}"#,
        )
        .unwrap();
        let remote = dir.join("contexts/meta/def456");
        std::fs::create_dir_all(&remote).unwrap();
        std::fs::write(
            remote.join("meta.json"),
            r#"{"Name":"prod","Endpoints":{"docker":{"Host":"tcp://10.0.0.5:2376"}}}"#,
        )
        .unwrap();
        let tls = dir.join("contexts/tls/def456/docker");
        std::fs::create_dir_all(&tls).unwrap();
        for f in ["ca.pem", "cert.pem", "key.pem"] {
            std::fs::write(tls.join(f), "").unwrap();
        }
        let list = contexts(&dir, "prod");
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "prod");
        assert!(list[0].tls.is_some() && list[0].current);
        assert!(list[1].tls.is_none() && !list[1].current);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn detects_sockets() {
        let home = temp("home");
        std::fs::create_dir_all(home.join(".docker/run")).unwrap();
        std::fs::write(home.join(".docker/run/docker.sock"), "").unwrap();
        let found = detected(&home);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "Local engine");
        assert!(found[0].host.ends_with(".docker/run/docker.sock"));
        std::fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn picks_an_engine() {
        let engine = |name: &str, current| Engine {
            name: name.into(),
            host: "unix:///x".into(),
            tls: None,
            current,
        };
        let list = vec![engine("default", false), engine("work", true)];
        assert_eq!(pick(&list, Some("default")).unwrap().name, "default");
        assert_eq!(pick(&list, Some("gone")).unwrap().name, "work");
        assert_eq!(pick(&list, None).unwrap().name, "work");
        assert!(engine("x", false).supported() == cfg!(unix));
    }
}
