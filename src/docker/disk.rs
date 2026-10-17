//! Disk usage (`docker system df`) and clean-up (`docker … prune`).

use std::collections::HashMap;

use bollard::query_parameters::{
    DataUsageOptions, PruneBuildOptions, PruneContainersOptions, PruneImagesOptions,
    PruneNetworksOptions, PruneVolumesOptions,
};

use super::{DockerClient, Resource, describe};

/// Space used by one kind of thing. Counts and sizes are what the engine
/// reports; older engines (API < 1.52) don't report these summaries.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct Usage {
    pub total_size: i64,
    pub reclaimable: i64,
    pub count: i64,
    pub active: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct DiskUsage {
    pub images: Option<Usage>,
    pub containers: Option<Usage>,
    pub volumes: Option<Usage>,
    pub build_cache: Option<Usage>,
}

impl DiskUsage {
    fn parts(&self) -> impl Iterator<Item = &Usage> {
        [
            &self.images,
            &self.containers,
            &self.volumes,
            &self.build_cache,
        ]
        .into_iter()
        .flatten()
    }

    pub fn total_size(&self) -> i64 {
        self.parts().map(|u| u.total_size).sum()
    }

    pub fn reclaimable(&self) -> i64 {
        self.parts().map(|u| u.reclaimable).sum()
    }

    /// Whether the engine reported anything at all.
    pub fn is_reported(&self) -> bool {
        self.parts().next().is_some()
    }
}

/// Something `docker … prune` can clean up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PruneKind {
    StoppedContainers,
    /// Untagged images no container uses.
    DanglingImages,
    /// Every image no container uses.
    UnusedImages,
    /// Named and anonymous volumes no container mounts.
    UnusedVolumes,
    UnusedNetworks,
    BuildCache,
}

impl PruneKind {
    /// "unused images", for messages.
    pub fn noun(self) -> &'static str {
        match self {
            PruneKind::StoppedContainers => "stopped containers",
            PruneKind::DanglingImages => "dangling images",
            PruneKind::UnusedImages => "unused images",
            PruneKind::UnusedVolumes => "unused volumes",
            PruneKind::UnusedNetworks => "unused networks",
            PruneKind::BuildCache => "build cache",
        }
    }

    /// The list that changes when this runs.
    pub fn affects(self) -> Resource {
        match self {
            PruneKind::StoppedContainers => Resource::Containers,
            PruneKind::DanglingImages | PruneKind::UnusedImages => Resource::Images,
            PruneKind::UnusedVolumes => Resource::Volumes,
            PruneKind::UnusedNetworks => Resource::Networks,
            PruneKind::BuildCache => Resource::Disk,
        }
    }
}

fn usage(
    total_size: Option<i64>,
    reclaimable: Option<i64>,
    count: Option<i64>,
    active: Option<i64>,
) -> Usage {
    Usage {
        total_size: total_size.unwrap_or_default(),
        reclaimable: reclaimable.unwrap_or_default(),
        count: count.unwrap_or_default(),
        active: active.unwrap_or_default(),
    }
}

impl DockerClient {
    pub async fn disk_usage(&self) -> Result<DiskUsage, String> {
        let df = self
            .docker
            .df(None::<DataUsageOptions>)
            .await
            .map_err(describe)?;
        Ok(DiskUsage {
            images: df
                .image_usage
                .map(|u| usage(u.total_size, u.reclaimable, u.total_count, u.active_count)),
            containers: df
                .container_usage
                .map(|u| usage(u.total_size, u.reclaimable, u.total_count, u.active_count)),
            volumes: df
                .volume_usage
                .map(|u| usage(u.total_size, u.reclaimable, u.total_count, u.active_count)),
            build_cache: df
                .build_cache_usage
                .map(|u| usage(u.total_size, u.reclaimable, u.total_count, u.active_count)),
        })
    }

    /// Cleans up one kind of thing. Returns the bytes freed.
    pub async fn prune(&self, kind: PruneKind) -> Result<i64, String> {
        let docker = &self.docker;
        let filter =
            |key: &str, value: &str| HashMap::from([(key.to_owned(), vec![value.to_owned()])]);
        let reclaimed = match kind {
            PruneKind::StoppedContainers => docker
                .prune_containers(None::<PruneContainersOptions>)
                .await
                .map(|r| r.space_reclaimed),
            PruneKind::DanglingImages | PruneKind::UnusedImages => {
                let dangling = if kind == PruneKind::DanglingImages {
                    "true"
                } else {
                    "false"
                };
                let opts = PruneImagesOptions {
                    filters: Some(filter("dangling", dangling)),
                };
                docker
                    .prune_images(Some(opts))
                    .await
                    .map(|r| r.space_reclaimed)
            }
            PruneKind::UnusedVolumes => {
                // Since API 1.42, only anonymous volumes go unless `all` is set.
                let opts = PruneVolumesOptions {
                    filters: Some(filter("all", "true")),
                };
                docker
                    .prune_volumes(Some(opts))
                    .await
                    .map(|r| r.space_reclaimed)
            }
            PruneKind::UnusedNetworks => docker
                .prune_networks(None::<PruneNetworksOptions>)
                .await
                .map(|_| Some(0)),
            PruneKind::BuildCache => {
                let opts = PruneBuildOptions {
                    all: Some(true),
                    ..Default::default()
                };
                docker
                    .prune_build(Some(opts))
                    .await
                    .map(|r| r.space_reclaimed)
            }
        };
        reclaimed.map(Option::unwrap_or_default).map_err(describe)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn totals_skip_unreported_parts() {
        let disk = DiskUsage {
            images: Some(Usage {
                total_size: 100,
                reclaimable: 40,
                ..Default::default()
            }),
            build_cache: Some(Usage {
                total_size: 50,
                reclaimable: 50,
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(disk.total_size(), 150);
        assert_eq!(disk.reclaimable(), 90);
        assert!(disk.is_reported());
        assert!(!DiskUsage::default().is_reported());
    }

    /// Talks to a real daemon: `cargo test -- --ignored`.
    #[tokio::test]
    #[ignore]
    async fn live_daemon_reports_disk_usage() {
        let client = DockerClient::connect().unwrap();
        let disk = client.disk_usage().await.unwrap();
        println!("{disk:?}");
        assert!(disk.is_reported());
    }
}
