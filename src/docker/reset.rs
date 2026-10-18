//! Reset Docker: remove everything for a clean start.

use bollard::query_parameters::{
    ListContainersOptions, RemoveContainerOptions, StopContainerOptions,
};

use super::{DockerClient, PruneKind, describe};

/// What the user types to confirm a reset.
pub const RESET_WORD: &str = "reset";

impl DockerClient {
    /// Stops and removes every container (helpers included), then cleans
    /// up all images, custom networks, the build cache and, unless
    /// `keep_volumes`, the volumes. Returns the bytes freed.
    pub async fn reset(&self, keep_volumes: bool) -> Result<i64, String> {
        let opts = ListContainersOptions {
            all: true,
            ..Default::default()
        };
        let containers = self
            .docker
            .list_containers(Some(opts))
            .await
            .map_err(describe)?;
        let removals = containers
            .iter()
            .filter_map(|c| c.id.as_deref())
            .map(|id| async move {
                // A graceful stop first, so databases shut down cleanly.
                let _ = self
                    .docker
                    .stop_container(id, None::<StopContainerOptions>)
                    .await;
                let opts = RemoveContainerOptions {
                    force: true,
                    // Anonymous volumes go with their container, unless kept.
                    v: !keep_volumes,
                    ..Default::default()
                };
                self.docker.remove_container(id, Some(opts)).await
            });
        let failed = futures_util::future::join_all(removals)
            .await
            .into_iter()
            .find_map(Result::err);
        if let Some(e) = failed {
            return Err(describe(e));
        }

        let mut kinds = vec![PruneKind::UnusedImages];
        if !keep_volumes {
            kinds.push(PruneKind::UnusedVolumes);
        }
        kinds.extend([PruneKind::UnusedNetworks, PruneKind::BuildCache]);
        let mut freed = 0;
        for kind in kinds {
            freed += self.prune(kind).await?;
        }
        Ok(freed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Wipes a real daemon: only run it on a throwaway engine,
    /// `cargo test reset -- --ignored`.
    #[tokio::test]
    #[ignore]
    async fn live_daemon_resets_everything() {
        let client = DockerClient::connect().unwrap();
        client.reset(true).await.unwrap();
        assert!(client.list_containers().await.unwrap().is_empty());
        assert!(client.list_images().await.unwrap().is_empty());
    }
}
