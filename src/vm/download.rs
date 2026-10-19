//! Getting the engine image: downloaded from the project's releases on
//! first use, checked against its published SHA-256, and unpacked into
//! the engine folder.

use std::io::Read;
use std::path::Path;

use sha2::{Digest, Sha256};

/// The engine image this build of Ferrodock uses.
pub const VERSION: &str = include_str!("../../engine/VERSION");

const ASSET: &str = "ferrodock-engine-arm64.tar.zst";

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
/// unpacks `kernel` and `rootfs.img` into `dir`.
pub fn install(dir: &Path, mut progress: impl FnMut(u64, Option<u64>)) -> Result<(), String> {
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
    unpack(&archive, dir)
}

/// Unpacks the zstd-compressed tar into `dir`, replacing what's there
/// only once everything is out.
pub fn unpack(archive: &[u8], dir: &Path) -> Result<(), String> {
    let staging = dir.with_extension("new");
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging).map_err(|e| e.to_string())?;
    let decoder = ruzstd::decoding::StreamingDecoder::new(archive).map_err(|e| e.to_string())?;
    tar::Archive::new(decoder)
        .unpack(&staging)
        .map_err(|e| format!("Couldn't unpack the engine: {e}"))?;
    for file in ["kernel", "rootfs.img"] {
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
        unpack(&archive, &dir).unwrap();
        assert_eq!(std::fs::read(dir.join("kernel")).unwrap(), b"kernel image");
        assert_eq!(std::fs::read(dir.join("rootfs.img")).unwrap(), b"root disk");
        assert!(!dir.join("old").exists());
        std::fs::remove_dir_all(&dir).unwrap();
        // An archive without the kernel is refused.
        assert!(
            unpack(
                &ruzstd::encoding::compress_to_vec(
                    &b""[..],
                    ruzstd::encoding::CompressionLevel::Fastest
                ),
                &dir
            )
            .is_err()
        );
    }
}
