//! The emulator's container: one, named, labelled, with its port
//! published and (if asked) its data in a volume.

use std::collections::HashMap;
use std::time::Duration;

use bollard::models::{ContainerCreateBody, HostConfig, PortBinding};
use bollard::query_parameters::{
    CreateContainerOptionsBuilder, InspectContainerOptions, RemoveContainerOptions,
    RemoveVolumeOptions, StartContainerOptions, StopContainerOptions,
};

use super::{
    CLOUD_CONTAINER, CLOUD_KEEP_LABEL, CLOUD_LABEL, CLOUD_VOLUME, CloudLabels, CloudSpec, DATA_DIR,
    INNER_PORT, api, image,
};
use crate::{DockerClient, describe};

/// How long a started emulator gets to answer: it's up in milliseconds,
/// so this is for a busy computer.
const START_TIMEOUT: Duration = Duration::from_secs(30);

fn not_found(e: &bollard::errors::Error) -> bool {
    matches!(
        e,
        bollard::errors::Error::DockerResponseServerError {
            status_code: 404,
            ..
        }
    )
}

fn body(spec: CloudSpec) -> ContainerCreateBody {
    ContainerCreateBody {
        image: Some(image::image()),
        // Without a volume it keeps nothing: said outright, so nothing is
        // written to an anonymous volume either.
        cmd: (!spec.keep_data).then(|| vec!["--ephemeral".to_owned()]),
        labels: Some(HashMap::from([
            (CLOUD_LABEL.to_owned(), spec.port.to_string()),
            (CLOUD_KEEP_LABEL.to_owned(), spec.keep_data.to_string()),
        ])),
        exposed_ports: Some(vec![INNER_PORT.to_owned()]),
        host_config: Some(HostConfig {
            // On all of the engine's addresses, like any published port:
            // the Mac engine's VM forwards only those to the Mac.
            port_bindings: Some(HashMap::from([(
                INNER_PORT.to_owned(),
                Some(vec![PortBinding {
                    host_ip: None,
                    host_port: Some(spec.port.to_string()),
                }]),
            )])),
            binds: spec
                .keep_data
                .then(|| vec![format!("{CLOUD_VOLUME}:{DATA_DIR}")]),
            ..HostConfig::default()
        }),
        ..ContainerCreateBody::default()
    }
}

impl DockerClient {
    /// What the emulator's container was made with, and its image; None
    /// if there's no container.
    async fn cloud_container(&self) -> Result<Option<(CloudLabels, String)>, String> {
        match self
            .docker
            .inspect_container(CLOUD_CONTAINER, None::<InspectContainerOptions>)
            .await
        {
            Ok(found) => {
                let config = found.config.unwrap_or_default();
                let labels = CloudLabels::from_labels(&config.labels.unwrap_or_default());
                Ok(Some((
                    labels.unwrap_or_default(),
                    config.image.unwrap_or_default(),
                )))
            }
            Err(e) if not_found(&e) => Ok(None),
            Err(e) => Err(describe(e)),
        }
    }

    /// Starts the emulator as `spec` says and waits until it answers.
    /// `step` hears what it's doing. A container made differently (another
    /// port, an older image) is replaced; its data stays in the volume.
    pub async fn start_cloud(
        &self,
        spec: CloudSpec,
        mut step: impl FnMut(&str),
    ) -> Result<(), String> {
        self.ensure_cloud_image(|| step("Downloading the emulator…"))
            .await?;
        step("Starting…");
        let wanted = CloudLabels {
            port: spec.port,
            keep_data: spec.keep_data,
        };
        match self.cloud_container().await? {
            Some((labels, used)) if labels == wanted && used == image::image() => {}
            existing => {
                if existing.is_some() {
                    self.remove_cloud_container().await?;
                }
                let options = CreateContainerOptionsBuilder::new()
                    .name(CLOUD_CONTAINER)
                    .build();
                self.docker
                    .create_container(Some(options), body(spec))
                    .await
                    .map_err(describe)?;
            }
        }
        self.docker
            .start_container(CLOUD_CONTAINER, None::<StartContainerOptions>)
            .await
            .map_err(|e| match describe(e) {
                // Docker's own words for a taken port are long and name
                // its proxy; say what to do instead.
                message if message.contains("port is already allocated") || message.contains("address already in use") => {
                    format!(
                        "Port {} is taken by something else. Stop that, or pick another port in Settings.",
                        spec.port
                    )
                }
                message => message,
            })?;
        let port = spec.port;
        let up = tokio::task::spawn_blocking(move || {
            let deadline = std::time::Instant::now() + START_TIMEOUT;
            while std::time::Instant::now() < deadline {
                if api::healthy(port) {
                    return true;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            false
        })
        .await
        .map_err(|e| e.to_string())?;
        if up {
            Ok(())
        } else {
            Err(format!(
                "The emulator started but doesn't answer on port {port}. Its logs are on the Containers page, under {CLOUD_CONTAINER}."
            ))
        }
    }

    /// Stops the emulator. Its container and data stay.
    pub async fn stop_cloud(&self) -> Result<(), String> {
        self.docker
            .stop_container(CLOUD_CONTAINER, None::<StopContainerOptions>)
            .await
            .or_else(|e| match e {
                // Already stopped, or never made.
                bollard::errors::Error::DockerResponseServerError {
                    status_code: 304 | 404,
                    ..
                } => Ok(()),
                e => Err(describe(e)),
            })
    }

    async fn remove_cloud_container(&self) -> Result<(), String> {
        self.docker
            .remove_container(
                CLOUD_CONTAINER,
                Some(RemoveContainerOptions {
                    force: true,
                    v: true,
                    ..Default::default()
                }),
            )
            .await
            .or_else(|e| {
                if not_found(&e) {
                    Ok(())
                } else {
                    Err(describe(e))
                }
            })
    }

    /// Removes the emulator's container and everything it kept.
    pub async fn remove_cloud(&self) -> Result<(), String> {
        self.remove_cloud_container().await?;
        self.docker
            .remove_volume(CLOUD_VOLUME, None::<RemoveVolumeOptions>)
            .await
            .or_else(|e| {
                if not_found(&e) {
                    Ok(())
                } else {
                    Err(describe(e))
                }
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_data_only_when_asked() {
        let kept = body(CloudSpec {
            port: 4570,
            keep_data: true,
        });
        let host = kept.host_config.unwrap();
        assert_eq!(
            host.binds.unwrap(),
            ["ferrodock-cloud-data:/var/lib/ferrocloud"]
        );
        assert_eq!(kept.cmd, None);
        let binding = &host.port_bindings.unwrap()["4566/tcp"];
        assert_eq!(
            binding.as_ref().unwrap()[0].host_port.as_deref(),
            Some("4570")
        );
        assert_eq!(kept.labels.unwrap()[CLOUD_LABEL], "4570");

        let empty = body(CloudSpec {
            port: 4566,
            keep_data: false,
        });
        assert_eq!(empty.cmd.unwrap(), ["--ephemeral"]);
        assert_eq!(empty.host_config.unwrap().binds, None);
    }

    /// Talks to a real daemon with a `ferrocloud:dev` image (or a pinned
    /// one): `cargo test -- --ignored`.
    #[tokio::test]
    #[ignore = "needs Docker and the emulator's image"]
    async fn runs_the_emulator() {
        let client = DockerClient::connect().unwrap();
        let spec = CloudSpec {
            port: 4599,
            keep_data: true,
        };
        client.remove_cloud().await.unwrap();
        let mut steps = Vec::new();
        client
            .start_cloud(spec, |s| steps.push(s.to_owned()))
            .await
            .unwrap();
        assert_eq!(steps.last().unwrap(), "Starting…");
        let info = tokio::task::spawn_blocking(|| super::super::info(4599))
            .await
            .unwrap()
            .unwrap();
        assert!(info.services.iter().any(|s| s.name == "sts"), "{info:?}");
        // Starting again changes nothing; another port replaces it.
        client.start_cloud(spec, |_| {}).await.unwrap();
        let moved = CloudSpec {
            port: 4598,
            keep_data: false,
        };
        client.start_cloud(moved, |_| {}).await.unwrap();
        assert!(api::healthy(4598) && !api::healthy(4599));
        tokio::task::spawn_blocking(|| super::super::reset(4598))
            .await
            .unwrap()
            .unwrap();
        client.stop_cloud().await.unwrap();
        client.stop_cloud().await.unwrap();
        assert!(!api::healthy(4598));
        client.remove_cloud().await.unwrap();
        client.remove_cloud().await.unwrap();
    }
}
