//! Getting Docker Compose when there's no `docker` command, as with only
//! Ferrodock's own engine: the official standalone `docker-compose`
//! program, downloaded once from Compose's releases into Ferrodock's
//! folder and checked against the SHA-256 below. Compose is Apache-2.0.
//! The app, `fdock` and the MCP server all use the same copy.

use std::io::Read;
use std::path::Path;

use sha2::{Digest, Sha256};

/// The Compose release Ferrodock downloads.
pub const VERSION: &str = "v5.6.0";

/// This platform's program and its SHA-256, from the release's
/// `.sha256` files. None where Compose publishes no build.
const ASSET: Option<(&str, &str)> = if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
    Some((
        "docker-compose-darwin-aarch64",
        "bd714a42b46e51757fc1121085b3096b9064e3f80d3ca5a991e33f44df4f43c9",
    ))
} else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
    Some((
        "docker-compose-darwin-x86_64",
        "c76e3abf5fde1aeb3bd7c7306ff15bef2246afc1f9e54832a0a050dd7cdfb2f8",
    ))
} else if cfg!(all(windows, target_arch = "x86_64")) {
    Some((
        "docker-compose-windows-x86_64.exe",
        "8aeaa0055e660e12cf07cd70680dc8333957fe6718528b91ec3da084eab62483",
    ))
} else if cfg!(all(windows, target_arch = "aarch64")) {
    Some((
        "docker-compose-windows-aarch64.exe",
        "73cdc73a03443cba37c7ae91aec12c7b86c0eb75a8f40f296aaa5159e66b29bc",
    ))
} else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
    Some((
        "docker-compose-linux-x86_64",
        "40343e21ca777173e69cff5dbafeb37c6f81f3b0d57d9e597f036e95eb63e76a",
    ))
} else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
    Some((
        "docker-compose-linux-aarch64",
        "733ec76717ceb59052a9609b9dadfb523b2df8eab57a54212872d10a58078ea2",
    ))
} else {
    None
};

/// Downloads Ferrodock's copy of Compose first when there's neither one
/// nor a `docker` command, calling `downloading` before it starts.
pub async fn ensure(downloading: impl FnOnce()) -> Result<(), String> {
    if super::compose_cli::program().is_some() {
        return Ok(());
    }
    let target = super::builtin::Paths::locate()
        .ok_or("No home folder for Docker Compose")?
        .compose();
    downloading();
    tokio::task::spawn_blocking(move || install(&target))
        .await
        .map_err(|e| e.to_string())?
}

/// Downloads Compose to `target`, checked, and makes it runnable.
pub fn install(target: &Path) -> Result<(), String> {
    let (asset, sha256) = ASSET.ok_or("Docker Compose has no build for this computer")?;
    let url = format!("https://github.com/docker/compose/releases/download/{VERSION}/{asset}");
    let mut response = ureq::get(&url).call().map_err(|e| format!("{url}: {e}"))?;
    let mut program = Vec::new();
    response
        .body_mut()
        .as_reader()
        .read_to_end(&mut program)
        .map_err(|e| format!("{url}: {e}"))?;
    save(&program, sha256, target)
}

/// Writes `program` to `target` if its SHA-256 is `sha256`: to a
/// temporary file first, so a half-written one is never run.
fn save(program: &[u8], sha256: &str, target: &Path) -> Result<(), String> {
    let digest = crate::hex(&Sha256::digest(program));
    if digest != sha256 {
        return Err("The Docker Compose download doesn't match its checksum".into());
    }
    let dir = target.parent().ok_or("no folder for Compose")?;
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let partial = target.with_extension("partial");
    std::fs::write(&partial, program).map_err(|e| format!("{}: {e}", partial.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&partial, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
    }
    std::fs::rename(&partial, target).map_err(|e| format!("{}: {e}", target.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_a_build_for_this_computer() {
        let (asset, sha256) = ASSET.unwrap();
        assert!(asset.starts_with("docker-compose-"));
        assert_eq!(sha256.len(), 64);
    }

    #[test]
    fn keeps_only_what_matches_the_checksum() {
        let dir = std::env::temp_dir().join(format!("ferrodock-compose-{}", std::process::id()));
        let target = dir.join("bin").join("docker-compose");
        let program = b"#!/bin/sh\necho compose\n";
        let good = crate::hex(&Sha256::digest(program));
        assert!(save(program, &"0".repeat(64), &target).is_err());
        assert!(!target.exists());
        save(program, &good, &target).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), program);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&target).unwrap().permissions().mode();
            assert_eq!(mode & 0o111, 0o111);
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// The real download, against the pinned checksum.
    #[test]
    #[ignore = "downloads Docker Compose"]
    fn downloads_compose() {
        let dir = std::env::temp_dir().join(format!("ferrodock-compose-dl-{}", std::process::id()));
        let target = dir.join("docker-compose");
        install(&target).unwrap();
        let output = std::process::Command::new(&target)
            .arg("version")
            .output()
            .unwrap();
        assert!(String::from_utf8_lossy(&output.stdout).contains(VERSION));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
