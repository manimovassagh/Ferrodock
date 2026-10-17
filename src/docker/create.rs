//! Creating volumes and networks, and connecting containers to networks.

use bollard::models::{
    ContainerUpdateBody, Ipam, IpamConfig, NetworkConnectRequest, NetworkCreateRequest,
    NetworkDisconnectRequest, RestartPolicy, VolumeCreateRequest,
};
use bollard::query_parameters::RenameContainerOptions;

use super::{Action, DockerClient, Resource, Restart, Task, describe};

/// A change that needs more than an ID to describe it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    CreateVolume {
        name: String,
        driver: String,
    },
    CreateNetwork {
        name: String,
        driver: String,
        /// e.g. `172.30.0.0/16`; empty lets Docker choose.
        subnet: String,
        /// No access to the outside world.
        internal: bool,
    },
    Connect {
        network: String,
        container: String,
    },
    Disconnect {
        network: String,
        container: String,
    },
    /// Give a container a new name.
    Rename {
        id: String,
        from: String,
        to: String,
    },
    /// Change a container's restart policy in place.
    SetRestart {
        id: String,
        name: String,
        policy: Restart,
    },
}

impl Change {
    /// The task that represents this change in progress and in messages.
    pub fn task(&self) -> Task {
        let (action, id, name) = match self {
            Change::CreateVolume { name, .. } => (
                Action::Create(Resource::Volumes),
                name.clone(),
                format!("volume {name}"),
            ),
            Change::CreateNetwork { name, .. } => (
                Action::Create(Resource::Networks),
                name.clone(),
                format!("network {name}"),
            ),
            Change::Connect { network, container } => (
                Action::Connect,
                format!("{network}+{container}"),
                format!("{container} to {network}"),
            ),
            Change::Disconnect { network, container } => (
                Action::Disconnect,
                format!("{network}+{container}"),
                format!("{container} from {network}"),
            ),
            Change::Rename { id, from, to } => {
                (Action::Rename, id.clone(), format!("{from} to {to}"))
            }
            Change::SetRestart { id, name, policy } => (
                Action::Update,
                id.clone(),
                format!("{name}'s restart policy to {}", policy.as_str()),
            ),
        };
        Task { action, id, name }
    }
}

impl DockerClient {
    pub async fn apply(&self, change: &Change) -> Result<(), String> {
        let docker = &self.docker;
        let result = match change.clone() {
            Change::CreateVolume { name, driver } => docker
                .create_volume(VolumeCreateRequest {
                    name: Some(name),
                    driver: (!driver.is_empty()).then_some(driver),
                    ..Default::default()
                })
                .await
                .map(drop),
            Change::CreateNetwork {
                name,
                driver,
                subnet,
                internal,
            } => docker
                .create_network(NetworkCreateRequest {
                    name,
                    driver: (!driver.is_empty()).then_some(driver),
                    internal: Some(internal),
                    ipam: (!subnet.trim().is_empty()).then(|| Ipam {
                        config: Some(vec![IpamConfig {
                            subnet: Some(subnet.trim().to_owned()),
                            ..Default::default()
                        }]),
                        ..Default::default()
                    }),
                    ..Default::default()
                })
                .await
                .map(drop),
            Change::Connect { network, container } => {
                docker
                    .connect_network(
                        &network,
                        NetworkConnectRequest {
                            container,
                            ..Default::default()
                        },
                    )
                    .await
            }
            Change::Rename { id, to, .. } => {
                docker
                    .rename_container(&id, RenameContainerOptions { name: to })
                    .await
            }
            Change::SetRestart { id, policy, .. } => {
                docker
                    .update_container(
                        &id,
                        ContainerUpdateBody {
                            restart_policy: Some(RestartPolicy {
                                name: Some(policy.engine_name()),
                                maximum_retry_count: None,
                            }),
                            ..Default::default()
                        },
                    )
                    .await
            }
            Change::Disconnect { network, container } => {
                docker
                    .disconnect_network(
                        &network,
                        NetworkDisconnectRequest {
                            container,
                            force: Some(false),
                        },
                    )
                    .await
            }
        };
        result.map_err(describe)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describes_changes() {
        let change = Change::Connect {
            network: "backend".into(),
            container: "web".into(),
        };
        let task = change.task();
        assert_eq!(task.action.past_tense(), "Connected");
        assert_eq!(task.name, "web to backend");
    }

    /// Talks to a real daemon: `cargo test -- --ignored`. Creates a
    /// volume and a network, connects a container, then cleans up.
    #[tokio::test]
    #[ignore]
    async fn live_daemon_creates_and_connects() {
        let client = DockerClient::connect().unwrap();
        let volume = Change::CreateVolume {
            name: "ferrodock-test-volume".into(),
            driver: String::new(),
        };
        client.apply(&volume).await.unwrap();
        let network = Change::CreateNetwork {
            name: "ferrodock-test-net".into(),
            driver: String::new(),
            subnet: String::new(),
            internal: true,
        };
        client.apply(&network).await.unwrap();
        let running = client
            .list_containers()
            .await
            .unwrap()
            .into_iter()
            .find(|c| c.is_running());
        if let Some(c) = running {
            let link = |connect: bool| {
                let (network, container) = ("ferrodock-test-net".to_owned(), c.id.clone());
                if connect {
                    Change::Connect { network, container }
                } else {
                    Change::Disconnect { network, container }
                }
            };
            client.apply(&link(true)).await.unwrap();
            client.apply(&link(false)).await.unwrap();
        }
        client
            .perform(Action::Remove(Resource::Networks), "ferrodock-test-net")
            .await
            .unwrap();
        client
            .perform(Action::Remove(Resource::Volumes), "ferrodock-test-volume")
            .await
            .unwrap();
    }

    /// Talks to a real daemon: `cargo test -- --ignored`. Runs a container,
    /// renames it, changes its restart policy, then removes it.
    #[tokio::test]
    #[ignore]
    async fn live_daemon_renames_and_updates() {
        use crate::docker::RunSpec;
        let client = DockerClient::connect().unwrap();
        let Some(image) = client.list_images().await.unwrap().into_iter().next() else {
            return;
        };
        let spec = RunSpec {
            image: image.id,
            name: "ferrodock-rename-a".into(),
            ..Default::default()
        };
        let id = client.run(&spec).await.unwrap();
        let rename = Change::Rename {
            id: id.clone(),
            from: "ferrodock-rename-a".into(),
            to: "ferrodock-rename-b".into(),
        };
        let restart = Change::SetRestart {
            id: id.clone(),
            name: "ferrodock-rename-b".into(),
            policy: Restart::UnlessStopped,
        };
        let renamed = client.apply(&rename).await;
        let updated = client.apply(&restart).await;
        let inspected = client.inspect(Resource::Containers, &id).await;
        let opts = bollard::query_parameters::RemoveContainerOptions {
            force: true,
            ..Default::default()
        };
        client
            .docker
            .remove_container(&id, Some(opts))
            .await
            .unwrap();
        renamed.unwrap();
        updated.unwrap();
        let inspected = inspected.unwrap();
        assert!(inspected.json.contains("ferrodock-rename-b"));
        assert_eq!(inspected.restart_policy.as_deref(), Some("unless-stopped"));
    }
}
