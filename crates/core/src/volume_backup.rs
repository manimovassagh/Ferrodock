//! Backing up, restoring and cloning volumes. Like browsing, it goes
//! through a helper container that's never started: Docker's archive API
//! reads and writes a stopped container's files, its volumes included.

use std::path::{Path, PathBuf};

use bollard::query_parameters::{DownloadFromContainerOptions, UploadToContainerOptions};
use futures_util::StreamExt;

use super::browse::VOLUME_ROOT;
use super::{DockerClient, describe};

impl DockerClient {
    /// Writes `volume`'s files to a new `<volume>.tar.gz` in `dir`.
    /// Returns its path.
    pub async fn back_up_volume(&self, volume: &str, dir: &Path) -> Result<PathBuf, String> {
        // Numbered before the extension, which has two parts.
        let path = std::iter::once(dir.join(format!("{volume}.tar.gz")))
            .chain((1..).map(|n| dir.join(format!("{volume} ({n}).tar.gz"))))
            .find(|p| !p.exists())
            .expect("some name is free");
        let result = self.back_up_to(volume, &path).await;
        if result.is_err() {
            let _ = std::fs::remove_file(&path);
        }
        result.map(|()| path)
    }

    async fn back_up_to(&self, volume: &str, path: &Path) -> Result<(), String> {
        let helper = self.volume_helper(volume).await?;
        let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
        let mut gz = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        // "/." for the folder's contents, so the backup holds the files
        // themselves and restores into any volume.
        let options = DownloadFromContainerOptions {
            path: format!("{VOLUME_ROOT}/."),
        };
        let mut stream = self.docker.download_from_container(&helper, Some(options));
        while let Some(chunk) = stream.next().await {
            std::io::Write::write_all(&mut gz, &chunk.map_err(describe)?)
                .map_err(|e| e.to_string())?;
        }
        gz.finish().map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Makes a new volume `name` holding the files in the backup at
    /// `archive` (a `.tar.gz` or `.tar`). Refuses to touch a volume that
    /// exists already.
    pub async fn restore_volume(&self, name: &str, archive: &Path) -> Result<(), String> {
        let body = tokio::fs::read(archive)
            .await
            .map_err(|e| format!("Couldn't open {}: {e}", archive.display()))?;
        self.new_volume(name).await?;
        let result = self.fill_volume(name, body).await;
        if result.is_err() {
            // Don't leave a half-filled volume behind.
            let _ = self
                .docker
                .remove_volume(name, None::<bollard::query_parameters::RemoveVolumeOptions>)
                .await;
        }
        result
    }

    /// Makes a new volume `to` with a copy of `from`'s files.
    pub async fn clone_volume(&self, from: &str, to: &str) -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!("ferrodock-clone-{}", std::process::id()));
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let result = async {
            let backup = self.back_up_volume(from, &dir).await?;
            self.restore_volume(to, &backup).await
        }
        .await;
        let _ = std::fs::remove_dir_all(&dir);
        result
    }

    async fn new_volume(&self, name: &str) -> Result<(), String> {
        if self.docker.inspect_volume(name).await.is_ok() {
            return Err(format!("a volume named {name} exists already"));
        }
        self.docker
            .create_volume(bollard::models::VolumeCreateRequest {
                name: Some(name.to_owned()),
                ..Default::default()
            })
            .await
            .map(drop)
            .map_err(describe)
    }

    async fn fill_volume(&self, name: &str, body: Vec<u8>) -> Result<(), String> {
        let helper = self.new_helper(name, false).await?;
        let options = UploadToContainerOptions {
            path: VOLUME_ROOT.to_owned(),
            ..Default::default()
        };
        // Docker unpacks gzip on its own.
        let result = self
            .docker
            .upload_to_container(&helper, Some(options), bollard::body_full(body.into()))
            .await
            .map_err(describe);
        let _ = self.remove_helpers(Some(name)).await;
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Change;

    /// The contents of the file at `path` in container `id`.
    async fn read(client: &DockerClient, id: &str, path: &str) -> Vec<u8> {
        let options = DownloadFromContainerOptions {
            path: path.to_owned(),
        };
        let mut bytes = Vec::new();
        let mut stream = client.docker.download_from_container(id, Some(options));
        while let Some(chunk) = stream.next().await {
            bytes.extend_from_slice(&chunk.unwrap());
        }
        let mut archive = tar::Archive::new(bytes.as_slice());
        let mut entry = archive.entries().unwrap().next().unwrap().unwrap();
        let mut data = Vec::new();
        std::io::Read::read_to_end(&mut entry, &mut data).unwrap();
        data
    }

    /// Talks to a real daemon: `cargo test -- --ignored`. Fills a volume,
    /// backs it up, restores and clones it, and reads the files back.
    #[tokio::test]
    #[ignore]
    async fn live_daemon_backs_up_restores_and_clones() {
        let client = DockerClient::connect().unwrap();
        let names = [
            "ferrodock-backup-test",
            "ferrodock-restore-test",
            "ferrodock-clone-test",
        ];
        let cleanup = || async {
            let _ = client.remove_helpers(None).await;
            for name in names {
                let _ = client
                    .docker
                    .remove_volume(name, None::<bollard::query_parameters::RemoveVolumeOptions>)
                    .await;
            }
        };
        cleanup().await;
        let [source, restored, cloned] = names;
        client
            .apply(&Change::CreateVolume {
                name: source.into(),
                driver: String::new(),
            })
            .await
            .unwrap();
        // Put files in through a writable helper.
        let mut tar = tar::Builder::new(Vec::new());
        for (name, data) in [
            ("notes.txt", &b"hello"[..]),
            ("sub/deep.bin", &[0u8, 1, 2][..]),
        ] {
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            tar.append_data(&mut header, name, data).unwrap();
        }
        client
            .fill_volume(source, tar.into_inner().unwrap())
            .await
            .unwrap();

        let dir = std::env::temp_dir().join(format!("ferrodock-backup-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let backup = client.back_up_volume(source, &dir).await.unwrap();
        assert!(backup.ends_with("ferrodock-backup-test.tar.gz"));
        let second = client.back_up_volume(source, &dir).await.unwrap();
        assert!(second.ends_with("ferrodock-backup-test (1).tar.gz"));
        client.restore_volume(restored, &backup).await.unwrap();
        client.clone_volume(source, cloned).await.unwrap();
        // Restoring over an existing volume is refused.
        assert!(client.restore_volume(cloned, &backup).await.is_err());

        for volume in [restored, cloned] {
            let helper = client.volume_helper(volume).await.unwrap();
            assert_eq!(read(&client, &helper, "/volume/notes.txt").await, b"hello");
            assert_eq!(
                read(&client, &helper, "/volume/sub/deep.bin").await,
                [0, 1, 2]
            );
        }
        std::fs::remove_dir_all(dir).unwrap();
        cleanup().await;
    }
}
