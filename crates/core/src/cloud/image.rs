//! The emulator's image: downloaded from the project's releases on first
//! use, checked against the SHA-256 built into this Ferrodock, and loaded
//! into Docker. An image that's already there is used as it is.

use std::io::Read;

use sha2::{Digest, Sha256};

use crate::DockerClient;

/// The image this build of Ferrodock uses and the SHA-256 of each of its
/// files. Built in rather than downloaded next to the image, so whoever
/// could replace a release's files still couldn't get a different image
/// accepted.
const PINNED: &str = include_str!("SHA256SUMS");

/// The image used when none is pinned: one built by hand, for working on
/// the emulator itself.
const UNPINNED: &str = "dev";

/// [`PINNED`] without its comments and blank lines.
fn lines(pinned: &str) -> impl Iterator<Item = &str> {
    pinned
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
}

/// The version pinned: the `cloud-v…` line.
fn version_in(pinned: &str) -> Option<&str> {
    lines(pinned).find_map(|line| line.strip_prefix("cloud-v"))
}

/// The pinned SHA-256 of `asset`.
fn checksum_in(pinned: &str, asset: &str) -> Option<String> {
    lines(pinned).find_map(|line| {
        let (digest, file) = line.split_once(char::is_whitespace)?;
        let digest = digest.to_lowercase();
        (file.trim() == asset
            && digest.len() == 64
            && digest.bytes().all(|b| b.is_ascii_hexdigit()))
        .then_some(digest)
    })
}

/// The version of the emulator's image this Ferrodock runs; `dev` when
/// none is published for it yet.
pub fn image_version() -> &'static str {
    version_in(PINNED).unwrap_or(UNPINNED)
}

/// The image's name in Docker.
pub(super) fn image() -> String {
    format!("ferrocloud:{}", image_version())
}

/// The release file for an engine of this architecture, as Docker names
/// it (`x86_64`, `aarch64`, `amd64`, `arm64`).
fn asset(architecture: &str) -> Option<&'static str> {
    match architecture {
        "x86_64" | "amd64" => Some("ferrocloud-amd64.tar.gz"),
        "aarch64" | "arm64" => Some("ferrocloud-arm64.tar.gz"),
        _ => None,
    }
}

/// Downloads `asset` of the pinned version, checked.
fn fetch(asset: &str) -> Result<Vec<u8>, String> {
    let version = version_in(PINNED).ok_or("This Ferrodock has no cloud emulator image yet.")?;
    let expected = checksum_in(PINNED, asset).ok_or_else(|| {
        format!("This Ferrodock has no checksum for {asset}, so it can't check it.")
    })?;
    let url = format!(
        "https://github.com/manimovassagh/Ferrodock/releases/download/cloud-v{version}/{asset}"
    );
    let mut response = ureq::get(&url).call().map_err(|e| format!("{url}: {e}"))?;
    let mut archive = Vec::new();
    response
        .body_mut()
        .as_reader()
        .read_to_end(&mut archive)
        .map_err(|e| format!("{url}: {e}"))?;
    if crate::hex(&Sha256::digest(&archive)) != expected {
        return Err(
            "The cloud emulator download isn't the one this Ferrodock expects (checksum mismatch). \
             Try again; if it keeps happening, please report it."
                .into(),
        );
    }
    Ok(archive)
}

impl DockerClient {
    /// Makes sure the emulator's image is in Docker, downloading it first
    /// if it isn't; `downloading` is called before that starts.
    pub(super) async fn ensure_cloud_image(
        &self,
        downloading: impl FnOnce(),
    ) -> Result<(), String> {
        if self.has_image(&image()).await {
            return Ok(());
        }
        let architecture = self
            .docker
            .info()
            .await
            .ok()
            .and_then(|info| info.architecture)
            .unwrap_or_else(|| std::env::consts::ARCH.to_owned());
        let asset = asset(&architecture).ok_or_else(|| {
            format!("The cloud emulator has no image for {architecture} engines.")
        })?;
        downloading();
        let archive = tokio::task::spawn_blocking(move || fetch(asset))
            .await
            .map_err(|e| e.to_string())??;
        // Docker reads a gzipped image as it is.
        self.load_image_bytes(archive).await?;
        if self.has_image(&image()).await {
            Ok(())
        } else {
            Err(format!("The download didn't hold {}.", image()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_pinned_image() {
        let digest = "a".repeat(64);
        let pinned = format!(
            "# note\n\ncloud-v0.2.1\n{digest}  ferrocloud-amd64.tar.gz\nnope  ferrocloud-arm64.tar.gz\n"
        );
        assert_eq!(version_in(&pinned), Some("0.2.1"));
        assert_eq!(
            checksum_in(&pinned, "ferrocloud-amd64.tar.gz"),
            Some(digest)
        );
        assert_eq!(checksum_in(&pinned, "ferrocloud-arm64.tar.gz"), None);
        assert_eq!(version_in("# nothing yet\n"), None);
    }

    #[test]
    fn pins_both_architectures_or_none() {
        match version_in(PINNED) {
            Some(version) => {
                assert_eq!(version.split('.').count(), 3, "{version}");
                for arch in ["x86_64", "aarch64"] {
                    let asset = asset(arch).unwrap();
                    assert!(
                        checksum_in(PINNED, asset).is_some(),
                        "no pinned checksum for {asset}"
                    );
                }
            }
            None => assert_eq!(image(), "ferrocloud:dev"),
        }
        assert_eq!(asset("amd64"), asset("x86_64"));
        assert_eq!(asset("riscv64"), None);
    }
}
