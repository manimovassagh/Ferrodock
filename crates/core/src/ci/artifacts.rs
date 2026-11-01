//! A run's artifacts: kept in a volume of the run while it's around, and
//! saved to a folder of this computer on request.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use bollard::models::{ContainerCreateBody, HostConfig};
use bollard::query_parameters::{
    CreateContainerOptionsBuilder, DownloadFromContainerOptions, ListContainersOptions,
    ListVolumesOptions, RemoveContainerOptions, RemoveVolumeOptions,
};
use futures_util::StreamExt;

use super::{CI_LABEL, actions};
use crate::{DockerClient, describe};

/// The volume holding a run's artifacts.
pub fn volume(run: u64) -> String {
    format!("ferrodock-ci-{run}-artifacts")
}

impl DockerClient {
    /// Saves the artifact `name` of `run` as `<name>.tar` in `dir`. Returns
    /// the file.
    pub async fn save_artifact(&self, run: u64, name: &str, dir: &Path) -> Result<PathBuf, String> {
        // Any image will do for a container that's never started: the
        // runner image is here already after a run, so nothing is pulled.
        let mut image = crate::debug_image();
        for ubuntu in ["24.04", "22.04"] {
            if self.has_image(&super::image::tag(ubuntu)).await {
                image = super::image::tag(ubuntu);
                break;
            }
        }
        if !self.has_image(&image).await {
            self.pull(&image, |_| {}).await?;
        }
        // A container that's never started is enough to copy files out of.
        let body = ContainerCreateBody {
            image: Some(image),
            labels: Some(HashMap::from([(CI_LABEL.to_owned(), run.to_string())])),
            host_config: Some(HostConfig {
                binds: Some(vec![format!("{}:{}", volume(run), actions::ARTIFACTS_DIR)]),
                ..HostConfig::default()
            }),
            ..ContainerCreateBody::default()
        };
        let helper = self
            .docker
            .create_container(Some(CreateContainerOptionsBuilder::new().build()), body)
            .await
            .map_err(describe)?
            .id;
        let saved = async {
            let path = crate::files::unique_path(dir, &format!("{name}.tar"));
            let mut file = std::fs::File::create(&path).map_err(|e| e.to_string())?;
            let source = format!("{}/{name}/.", actions::ARTIFACTS_DIR);
            let mut stream = self.docker.download_from_container(
                &helper,
                Some(DownloadFromContainerOptions { path: source }),
            );
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(describe)?;
                std::io::Write::write_all(&mut file, &chunk).map_err(|e| e.to_string())?;
            }
            Ok(path)
        }
        .await;
        let force = RemoveContainerOptions {
            force: true,
            ..Default::default()
        };
        let _ = self.docker.remove_container(&helper, Some(force)).await;
        saved
    }

    /// Removes the artifacts of a run.
    pub async fn remove_artifacts(&self, run: u64) {
        let _ = self
            .docker
            .remove_volume(&volume(run), None::<RemoveVolumeOptions>)
            .await;
    }

    /// Removes what runs of an earlier session left: their stopped
    /// containers and their artifacts (which only that session could
    /// show). The tool and dependency caches stay. Anything still in use,
    /// e.g. by `fdock` running a workflow, is left alone.
    pub async fn remove_ci_leftovers(&self) {
        let options = ListContainersOptions {
            all: true,
            filters: Some(HashMap::from([(
                "label".to_owned(),
                vec![CI_LABEL.to_owned()],
            )])),
            ..Default::default()
        };
        if let Ok(list) = self.docker.list_containers(Some(options)).await {
            for c in list {
                let running = c.state.is_some_and(|s| s.to_string() == "running");
                if let (Some(id), false) = (c.id, running) {
                    let force = RemoveContainerOptions {
                        force: true,
                        ..Default::default()
                    };
                    let _ = self.docker.remove_container(&id, Some(force)).await;
                }
            }
        }
        let Ok(volumes) = self.docker.list_volumes(None::<ListVolumesOptions>).await else {
            return;
        };
        for v in volumes.volumes.unwrap_or_default() {
            if v.name.starts_with("ferrodock-ci-") && v.name.ends_with("-artifacts") {
                let _ = self
                    .docker
                    .remove_volume(&v.name, None::<RemoveVolumeOptions>)
                    .await;
            }
        }
    }
}
