//! Creating volumes and networks, connecting containers to networks, and
//! changing containers: rename, restart policy, limits, edit & recreate.

use bollard::models::{
    ContainerUpdateBody, Ipam, IpamConfig, NetworkConnectRequest, NetworkCreateRequest,
    NetworkDisconnectRequest, RestartPolicy, VolumeCreateRequest,
};
use bollard::query_parameters::{
    RemoveContainerOptions, RenameContainerOptions, StartContainerOptions, StopContainerOptions,
};

use super::{Action, DockerClient, Resource, Restart, RunSpec, Task, describe};

/// A change that needs more than an ID to describe it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    CreateVolume {
        name: String,
        driver: String,
    },
    /// Save a volume's files to a `.tar.gz` in the Downloads folder.
    BackUpVolume {
        name: String,
    },
    /// Make a new volume from a backup.
    RestoreVolume {
        name: String,
        archive: std::path::PathBuf,
    },
    /// Make a new volume `to` with a copy of `from`'s files.
    CloneVolume {
        from: String,
        to: String,
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
    /// Change a container's CPU and memory limits in place. 0 leaves a
    /// limit as it is: the Engine can't remove one without a recreate.
    SetLimits {
        id: String,
        name: String,
        nano_cpus: i64,
        memory: i64,
    },
    /// Replace a container with a new one made from `spec`. The old one is
    /// put back if the new one can't start.
    Recreate {
        id: String,
        name: String,
        running: bool,
        spec: Box<RunSpec>,
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
            Change::BackUpVolume { name } => (
                Action::Save,
                name.clone(),
                format!("volume {name} to your Downloads folder"),
            ),
            Change::RestoreVolume { name, archive } => (
                Action::Create(Resource::Volumes),
                name.clone(),
                format!(
                    "volume {name} from {}",
                    archive
                        .file_name()
                        .map(|f| f.to_string_lossy())
                        .unwrap_or_default()
                ),
            ),
            Change::CloneVolume { from, to } => (
                Action::Create(Resource::Volumes),
                to.clone(),
                format!("volume {to} from {from}"),
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
            Change::SetLimits { id, name, .. } => {
                (Action::Update, id.clone(), format!("{name}'s limits"))
            }
            Change::Recreate { id, name, .. } => (Action::Recreate, id.clone(), name.clone()),
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
            Change::SetLimits {
                id,
                nano_cpus,
                memory,
                ..
            } => {
                docker
                    .update_container(
                        &id,
                        ContainerUpdateBody {
                            nano_cpus: (nano_cpus > 0).then_some(nano_cpus),
                            memory: (memory > 0).then_some(memory),
                            // Unlimited swap, so a lower memory limit never
                            // clashes with the swap limit set before.
                            memory_swap: (memory > 0).then_some(-1),
                            ..Default::default()
                        },
                    )
                    .await
            }
            Change::Recreate {
                id,
                name,
                running,
                spec,
            } => return self.recreate(&id, &name, running, &spec).await,
            Change::BackUpVolume { name } => {
                return self
                    .back_up_volume(&name, &super::downloads_dir())
                    .await
                    .map(drop);
            }
            Change::RestoreVolume { name, archive } => {
                return self.restore_volume(&name, &archive).await;
            }
            Change::CloneVolume { from, to } => return self.clone_volume(&from, &to).await,
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

    /// Moves the old container aside (renamed and stopped), starts the new
    /// one, then removes the old one. If the new one fails, the old one gets
    /// its name back and is started again if it was running.
    async fn recreate(
        &self,
        id: &str,
        name: &str,
        running: bool,
        spec: &RunSpec,
    ) -> Result<(), String> {
        let docker = &self.docker;
        let aside = format!("{name}-replaced-{}", &id[..id.len().min(8)]);
        let rename = |to: &str| RenameContainerOptions {
            name: to.to_owned(),
        };
        docker
            .rename_container(id, rename(&aside))
            .await
            .map_err(describe)?;
        if running
            && let Err(e) = docker
                .stop_container(id, None::<StopContainerOptions>)
                .await
        {
            let _ = docker.rename_container(id, rename(name)).await;
            return Err(describe(e));
        }
        match self.run(spec).await {
            Ok(_) => docker
                .remove_container(id, None::<RemoveContainerOptions>)
                .await
                .map_err(|e| {
                    format!(
                        "the new container runs, but the old one is still there as {aside}: {}",
                        describe(e)
                    )
                }),
            Err(e) => {
                let _ = docker.rename_container(id, rename(name)).await;
                if running {
                    let _ = docker
                        .start_container(id, None::<StartContainerOptions>)
                        .await;
                }
                Err(e)
            }
        }
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
        use crate::RunSpec;
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

    /// Talks to a real daemon: `cargo test -- --ignored`. Runs a container,
    /// changes its limits, recreates it with a new port, then removes it.
    #[tokio::test]
    #[ignore]
    async fn live_daemon_sets_limits_and_recreates() {
        use crate::RunSpec;
        let client = DockerClient::connect().unwrap();
        let Some(image) = client.list_images().await.unwrap().into_iter().next() else {
            return;
        };
        let spec = RunSpec {
            image: image.id,
            name: "ferrodock-recreate".into(),
            command: "sleep 300".into(),
            ..Default::default()
        };
        let id = client.run(&spec).await.unwrap();
        let limits = Change::SetLimits {
            id: id.clone(),
            name: spec.name.clone(),
            nano_cpus: 500_000_000,
            memory: 64 << 20,
        };
        let limited = client.apply(&limits).await;
        let inspected = client.inspect(Resource::Containers, &id).await;
        let edited = RunSpec {
            nano_cpus: 1_000_000_000,
            ports: vec![(18089, 80)],
            ..spec.clone()
        };
        let recreate = Change::Recreate {
            id: id.clone(),
            name: spec.name.clone(),
            running: true,
            spec: Box::new(edited),
        };
        let recreated = client.apply(&recreate).await;
        let containers = client.list_containers().await.unwrap();
        let new = containers.iter().find(|c| c.name == "ferrodock-recreate");
        let new_id = new.map(|c| c.id.clone());
        let old_gone = containers.iter().all(|c| c.id != id);
        let new_settings = match &new_id {
            Some(new_id) => Some(client.inspect(Resource::Containers, new_id).await),
            None => None,
        };
        let opts = || bollard::query_parameters::RemoveContainerOptions {
            force: true,
            ..Default::default()
        };
        for target in [Some(id.clone()), new_id.clone()].into_iter().flatten() {
            let _ = client.docker.remove_container(&target, Some(opts())).await;
        }
        limited.unwrap();
        let run = inspected.unwrap().run;
        assert_eq!((run.nano_cpus, run.memory), (500_000_000, 64 << 20));
        recreated.unwrap();
        assert!(old_gone, "the old container should be removed");
        let new_run = new_settings.unwrap().unwrap().run;
        assert_eq!(new_run.ports, vec![(18089, 80)]);
        assert_eq!(new_run.nano_cpus, 1_000_000_000);
    }

    /// Talks to a real daemon: `cargo test -- --ignored`. A recreate whose
    /// new container can't be made puts the old one back as it was.
    #[tokio::test]
    #[ignore]
    async fn live_daemon_recreate_rolls_back() {
        use crate::RunSpec;
        let client = DockerClient::connect().unwrap();
        let Some(image) = client.list_images().await.unwrap().into_iter().next() else {
            return;
        };
        let spec = RunSpec {
            image: image.id,
            name: "ferrodock-rollback".into(),
            command: "sleep 300".into(),
            ..Default::default()
        };
        let id = client.run(&spec).await.unwrap();
        let broken = RunSpec {
            image: "ferrodock/no-such-image:never".into(),
            ..spec.clone()
        };
        let recreate = Change::Recreate {
            id: id.clone(),
            name: spec.name.clone(),
            running: true,
            spec: Box::new(broken),
        };
        let result = client.apply(&recreate).await;
        let containers = client.list_containers().await.unwrap();
        let old = containers.iter().find(|c| c.id == id).cloned();
        let opts = bollard::query_parameters::RemoveContainerOptions {
            force: true,
            ..Default::default()
        };
        let _ = client.docker.remove_container(&id, Some(opts)).await;
        assert!(result.is_err());
        let old = old.expect("the old container is kept");
        assert_eq!(old.name, "ferrodock-rollback");
        assert!(old.is_running());
    }
}
