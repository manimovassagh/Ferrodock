//! Getting the engine image: downloaded from the project's releases on
//! first use, checked against its published SHA-256, and unpacked: on a
//! Mac the kernel and root disk, on Windows the distro's files for WSL.

// Tested everywhere, used only where there's an engine.
#![cfg_attr(not(engine), allow(dead_code))]

use std::io::Read;
use std::path::Path;

use sha2::{Digest, Sha256};

/// The engine image this build of Ferrodock uses.
pub const VERSION: &str = include_str!("../../engine/VERSION");

/// The image for this platform: kernel and root disk for the Mac's VM
/// (Apple Silicon or Intel), a root filesystem for WSL, or the engine's
/// programs for Linux.
pub const ASSET: &str = if cfg!(windows) {
    "ferrodock-engine-wsl-amd64.tar.zst"
} else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
    "ferrodock-engine-arm64.tar.zst"
} else if cfg!(target_os = "macos") {
    "ferrodock-engine-amd64.tar.zst"
} else if cfg!(target_arch = "aarch64") {
    "ferrodock-engine-linux-arm64.tar.zst"
} else {
    "ferrodock-engine-linux-amd64.tar.zst"
};

fn base_url() -> String {
    format!(
        "https://github.com/manimovassagh/Ferrodock/releases/download/engine-v{}",
        VERSION.trim()
    )
}

/// The hex digest at the start of a `sha256sum` line.
pub fn parse_checksum(line: &str) -> Option<String> {
    let digest = line.split_whitespace().next()?.to_lowercase();
    (digest.len() == 64 && digest.bytes().all(|b| b.is_ascii_hexdigit())).then_some(digest)
}

/// Downloads the image, calling `progress` with (done, total) bytes, and
/// checks it against its published checksum. The image is still
/// zstd-compressed.
pub fn fetch(mut progress: impl FnMut(u64, Option<u64>)) -> Result<Vec<u8>, String> {
    let get = |url: &str| ureq::get(url).call().map_err(|e| format!("{url}: {e}"));
    let checksum = get(&format!("{}/{ASSET}.sha256", base_url()))?
        .body_mut()
        .read_to_string()
        .map_err(|e| e.to_string())?;
    let expected = parse_checksum(&checksum).ok_or("the engine's checksum file is malformed")?;

    let mut response = get(&format!("{}/{ASSET}", base_url()))?;
    let total = response.body().content_length();
    let mut reader = response.body_mut().as_reader();
    let mut archive = Vec::with_capacity(total.unwrap_or(0) as usize);
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 256 * 1024];
    loop {
        let n = reader.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
        archive.extend_from_slice(&buffer[..n]);
        progress(archive.len() as u64, total);
    }
    let actual = format!("{:x}", hasher.finalize());
    if actual != expected {
        return Err("The engine download is damaged (checksum mismatch). Try again.".into());
    }
    Ok(archive)
}

/// Uncompresses the image into the file at `path` (on Windows, the tar
/// that WSL imports as the distro).
#[cfg_attr(not(windows), allow(dead_code))]
pub fn decompress(archive: &[u8], path: &Path) -> Result<(), String> {
    let mut decoder =
        ruzstd::decoding::StreamingDecoder::new(archive).map_err(|e| e.to_string())?;
    let mut file = std::fs::File::create(path).map_err(|e| format!("{}: {e}", path.display()))?;
    std::io::copy(&mut decoder, &mut file)
        .map(drop)
        .map_err(|e| format!("Couldn't unpack the engine: {e}"))
}

/// Unpacks the zstd-compressed tar into `dir`, replacing what's there
/// only once everything is out, and `needs` is there (the Mac's kernel
/// and root disk, or Linux's engine programs).
#[cfg_attr(windows, allow(dead_code))]
pub fn unpack(archive: &[u8], dir: &Path, needs: &[&str]) -> Result<(), String> {
    let staging = dir.with_extension("new");
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging).map_err(|e| e.to_string())?;
    let decoder = ruzstd::decoding::StreamingDecoder::new(archive).map_err(|e| e.to_string())?;
    tar::Archive::new(decoder)
        .unpack(&staging)
        .map_err(|e| format!("Couldn't unpack the engine: {e}"))?;
    for file in needs {
        if !staging.join(file).is_file() {
            return Err(format!("The engine image has no {file}"));
        }
    }
    let _ = std::fs::remove_dir_all(dir);
    std::fs::rename(&staging, dir).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_sha256sum_lines() {
        let digest = "a".repeat(64);
        assert_eq!(
            parse_checksum(&format!("{digest}  ferrodock-engine-arm64.tar.zst\n")),
            Some(digest)
        );
        assert_eq!(parse_checksum("nope file"), None);
        assert_eq!(parse_checksum(""), None);
        assert_eq!(VERSION.trim().split('.').count(), 3);
    }

    /// A zstd-compressed tar like the published one, unpacked for real.
    #[test]
    fn unpacks_the_image() {
        let mut tarball = tar::Builder::new(Vec::new());
        for (name, body) in [
            ("kernel", &b"kernel image"[..]),
            ("rootfs.img", b"root disk"),
        ] {
            let mut header = tar::Header::new_gnu();
            header.set_size(body.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            tarball.append_data(&mut header, name, body).unwrap();
        }
        let archive = ruzstd::encoding::compress_to_vec(
            tarball.into_inner().unwrap().as_slice(),
            ruzstd::encoding::CompressionLevel::Fastest,
        );
        let dir = std::env::temp_dir().join(format!("ferrodock-engine-{}", std::process::id()));
        // What a previous version left is replaced.
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("old"), "old").unwrap();
        unpack(&archive, &dir, &["kernel", "rootfs.img"]).unwrap();
        assert_eq!(std::fs::read(dir.join("kernel")).unwrap(), b"kernel image");
        assert_eq!(std::fs::read(dir.join("rootfs.img")).unwrap(), b"root disk");
        assert!(!dir.join("old").exists());
        // Or just uncompressed, as WSL takes it.
        let tar = dir.with_extension("tar");
        decompress(&archive, &tar).unwrap();
        assert_eq!(
            tar::Archive::new(std::fs::File::open(&tar).unwrap())
                .entries()
                .unwrap()
                .count(),
            2
        );
        std::fs::remove_file(&tar).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        // An archive without the kernel is refused.
        assert!(
            unpack(
                &ruzstd::encoding::compress_to_vec(
                    &b""[..],
                    ruzstd::encoding::CompressionLevel::Fastest
                ),
                &dir,
                &["kernel"]
            )
            .is_err()
        );
    }
}
