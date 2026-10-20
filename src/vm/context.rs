//! The `ferrodock` Docker context: the engine offered to the `docker`
//! command and other tools, for `docker context use ferrodock`. Written
//! as `docker context create` writes it, so the Docker CLI needn't be
//! installed. It's never made the current context: that stays the user's
//! choice.

// Tested everywhere, used only where there's an engine.
#![cfg_attr(not(engine), allow(dead_code))]

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use super::CONTEXT;

/// Where the context `name` is kept in the Docker CLI's config folder
/// `dir`: `contexts/meta/<SHA-256 of the name>`.
fn folder(dir: &Path, name: &str) -> PathBuf {
    dir.join("contexts")
        .join("meta")
        .join(format!("{:x}", Sha256::digest(name.as_bytes())))
}

/// The context's `meta.json`.
fn meta(name: &str, host: &str) -> String {
    serde_json::json!({
        "Name": name,
        "Metadata": { "Description": "Ferrodock" },
        "Endpoints": { "docker": { "Host": host, "SkipTLSVerify": false } },
    })
    .to_string()
}

/// Adds the context to `dir`, or points it at `host` again.
fn write(dir: &Path, host: &str) -> Result<(), String> {
    let folder = folder(dir, CONTEXT);
    let file = folder.join("meta.json");
    let text = meta(CONTEXT, host);
    if std::fs::read_to_string(&file).is_ok_and(|t| t == text) {
        return Ok(());
    }
    std::fs::create_dir_all(&folder)
        .and_then(|()| std::fs::write(&file, text))
        .map_err(|e| format!("{}: {e}", file.display()))
}

/// Adds the `ferrodock` context for the engine at `host`.
pub fn add(host: &str) -> Result<(), String> {
    let dir = ferrodock_core::docker_dir().ok_or("No home folder")?;
    write(&dir, host)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_what_the_docker_cli_reads() {
        let dir = std::env::temp_dir().join(format!("ferrodock-context-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let host = "unix:///Users/me/.ferrodock/docker.sock";
        write(&dir, host).unwrap();
        // `docker context create ferrodock` keeps it here.
        let file = dir
            .join("contexts/meta")
            .join("d723a9f13f0e52c665928ba22d6031999fe4913fa1d37874a9362bba579f411a")
            .join("meta.json");
        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(json["Name"], "ferrodock");
        assert_eq!(json["Metadata"]["Description"], "Ferrodock");
        assert_eq!(json["Endpoints"]["docker"]["Host"], host);
        assert_eq!(json["Endpoints"]["docker"]["SkipTLSVerify"], false);
        // Again, pointing somewhere else: still one context.
        write(&dir, "npipe:////./pipe/ferrodock_engine").unwrap();
        assert_eq!(
            std::fs::read_dir(dir.join("contexts/meta"))
                .unwrap()
                .count(),
            1
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
