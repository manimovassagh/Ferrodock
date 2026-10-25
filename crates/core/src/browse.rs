//! Browsing a volume's files. Docker can only copy files out of a
//! container, so a helper container is created with the volume mounted
//! read-only at [`VOLUME_ROOT`]. It's never started: the archive API reads
//! a stopped container's files, mounts included. Helpers carry
//! [`HELPER_LABEL`], are hidden from the container list and removed when
//! no longer needed.

use std::collections::HashMap;

use bollard::models::{ContainerCreateBody, HostConfig};
use bollard::query_parameters::{
    CreateContainerOptions, ListContainersOptions, RemoveContainerOptions,
};

use super::{DockerClient, describe};

/// Marks Ferrodock's helper containers; the value is the volume's name.
pub const HELPER_LABEL: &str = "dev.ferrodock.browse-volume";
/// Where the volume is mounted in the helper.
pub const VOLUME_ROOT: &str = "/volume";
/// Pulled when there's no image at all to base a helper on.
const FALLBACK_IMAGE: &str = "busybox:latest";

impl DockerClient {
    /// A helper container with `volume` mounted, made if needed. Returns its
    /// ID.
    pub async fn volume_helper(&self, volume: &str) -> Result<String, String> {
        if let Some(id) = self.helpers(Some(volume)).await?.into_iter().next() {
            return Ok(id);
        }
        self.new_helper(volume, true).await
    }

    /// A new helper container with `volume` mounted, writable unless
    /// `read_only`. Returns its ID.
    pub(super) async fn new_helper(&self, volume: &str, read_only: bool) -> Result<String, String> {
        // Any image will do, since the helper never runs; the smallest one
        // keeps it cheap.
        let mut images = self.list_images().await?;
        images.sort_by_key(|i| i.size);
        let image = match images.first() {
            Some(image) => image.id.clone(),
            None => {
                self.pull(FALLBACK_IMAGE, |_| {}).await?;
                FALLBACK_IMAGE.to_owned()
            }
        };
        let body = ContainerCreateBody {
            image: Some(image),
            // Never run, but an image without a command can't be created.
            cmd: Some(vec!["ferrodock-browse".into()]),
            labels: Some(HashMap::from([(
                HELPER_LABEL.to_owned(),
                volume.to_owned(),
            )])),
            host_config: Some(HostConfig {
                binds: Some(vec![format!(
                    "{volume}:{VOLUME_ROOT}:{}",
                    if read_only { "ro" } else { "rw" }
                )]),
                ..Default::default()
            }),
            ..Default::default()
        };
        let created = self
            .docker
            .create_container(None::<CreateContainerOptions>, body)
            .await
            .map_err(describe)?;
        Ok(created.id)
    }

    /// Removes the helpers of `volume`, or all of them.
    pub async fn remove_helpers(&self, volume: Option<&str>) -> Result<(), String> {
        for id in self.helpers(volume).await? {
            let opts = RemoveContainerOptions {
                force: true,
                ..Default::default()
            };
            self.docker
                .remove_container(&id, Some(opts))
                .await
                .map_err(describe)?;
        }
        Ok(())
    }

    async fn helpers(&self, volume: Option<&str>) -> Result<Vec<String>, String> {
        let label = match volume {
            Some(volume) => format!("{HELPER_LABEL}={volume}"),
            None => HELPER_LABEL.to_owned(),
        };
        let opts = ListContainersOptions {
            all: true,
            filters: Some(HashMap::from([("label".to_owned(), vec![label])])),
            ..Default::default()
        };
        let list = self
            .docker
            .list_containers(Some(opts))
            .await
            .map_err(describe)?;
        Ok(list.into_iter().filter_map(|c| c.id).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Talks to a real daemon: `cargo test -- --ignored`. Makes a volume
    /// with a file in it (through a container), browses it with a helper,
    /// and checks the helper is hidden and cleaned up.
    #[tokio::test]
    #[ignore]
    async fn live_daemon_browses_a_volume() {
        use crate::{Action, Change, Preview, Resource};
        let client = DockerClient::connect().unwrap();
        if client.list_images().await.unwrap().is_empty() {
            return;
        }
        let volume = "ferrodock-browse-test";
        client
            .apply(&Change::CreateVolume {
                name: volume.into(),
                driver: String::new(),
            })
            .await
            .unwrap();
        let helper = client.volume_helper(volume).await.unwrap();
        let dir = std::env::temp_dir().join(format!("ferrodock-browse-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("hello.txt"), "hi from a volume\n").unwrap();
        // Uploading needs a writable mount, so the file goes in through a
        // second (unstarted) container that mounts it read-write.
        let rw = client
            .docker
            .create_container(
                None::<CreateContainerOptions>,
                ContainerCreateBody {
                    image: Some(client.list_images().await.unwrap()[0].id.clone()),
                    cmd: Some(vec!["x".into()]),
                    host_config: Some(HostConfig {
                        binds: Some(vec![format!("{volume}:/data")]),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            )
            .await
            .unwrap()
            .id;
        let uploaded = client
            .upload_files(&rw, "/data", &[dir.join("hello.txt")])
            .await;
        let _ = client
            .perform(Action::Remove(Resource::Containers), &rw)
            .await;

        let listing = client.list_files(&helper, VOLUME_ROOT).await;
        let preview = client
            .preview_file(&helper, &format!("{VOLUME_ROOT}/hello.txt"))
            .await;
        let again = client.volume_helper(volume).await.unwrap();
        let hidden = client
            .list_containers()
            .await
            .unwrap()
            .iter()
            .all(|c| c.id != helper);
        // Removing the volume takes its helper with it.
        let removed = client
            .perform(Action::Remove(Resource::Volumes), volume)
            .await;
        let left = client.helpers(Some(volume)).await.unwrap();
        std::fs::remove_dir_all(&dir).unwrap();

        uploaded.unwrap();
        let listing = listing.unwrap();
        assert!(listing.entries.iter().any(|e| e.name == "hello.txt"));
        assert_eq!(preview.unwrap(), Preview::Text("hi from a volume\n".into()));
        assert_eq!(again, helper, "the helper is reused");
        assert!(hidden, "helpers don't show in the container list");
        removed.unwrap();
        assert!(left.is_empty());
    }
}
