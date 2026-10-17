//! Things Ferrodock can do: container lifecycle, removing any resource,
//! and cleaning up unused things.

use bollard::query_parameters::{
    RemoveContainerOptions, RemoveImageOptions, RemoveVolumeOptions, RestartContainerOptions,
    StartContainerOptions, StopContainerOptions,
};

use super::{DockerClient, PruneKind, Resource, describe};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Start,
    Stop,
    Restart,
    Pause,
    Unpause,
    Remove(Resource),
    /// Clean up a kind of unused thing (`docker … prune`).
    Prune(PruneKind),
    /// Pull an image; the task's ID is the image reference. Streamed by
    /// the worker, not run through [`DockerClient::perform`].
    Pull,
    /// Create and start a container from an image (also worker-driven).
    Run,
    /// Create a volume or network (worker-driven, see [`super::Change`]).
    Create(Resource),
    /// Connect a container to a network.
    Connect,
    /// Disconnect a container from a network.
    Disconnect,
    /// Rename a container.
    Rename,
    /// Change a container's settings in place.
    Update,
}

impl Action {
    /// "start", "remove", … for error messages.
    pub fn verb(self) -> &'static str {
        match self {
            Action::Start => "start",
            Action::Stop => "stop",
            Action::Restart => "restart",
            Action::Pause => "pause",
            Action::Unpause => "resume",
            Action::Remove(_) => "remove",
            Action::Prune(_) => "clean up",
            Action::Pull => "pull",
            Action::Run => "run",
            Action::Create(_) => "create",
            Action::Connect => "connect",
            Action::Disconnect => "disconnect",
            Action::Rename => "rename",
            Action::Update => "change",
        }
    }

    /// "Started", "Removed", … for success messages.
    pub fn past_tense(self) -> &'static str {
        match self {
            Action::Start => "Started",
            Action::Stop => "Stopped",
            Action::Restart => "Restarted",
            Action::Pause => "Paused",
            Action::Unpause => "Resumed",
            Action::Remove(_) | Action::Prune(_) => "Removed",
            Action::Pull => "Pulled",
            Action::Run => "Started",
            Action::Create(_) => "Created",
            Action::Connect => "Connected",
            Action::Disconnect => "Disconnected",
            Action::Rename => "Renamed",
            Action::Update => "Changed",
        }
    }

    /// The list that changes when this action runs.
    pub fn affects(self) -> Resource {
        match self {
            Action::Remove(resource) => resource,
            Action::Prune(kind) => kind.affects(),
            Action::Pull => Resource::Images,
            Action::Create(resource) => resource,
            Action::Connect | Action::Disconnect => Resource::Networks,
            _ => Resource::Containers,
        }
    }
}

/// An action on one item. `id` is what the Engine expects (a volume's name,
/// everything else's ID); `name` is what the user sees.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    pub action: Action,
    pub id: String,
    pub name: String,
}

impl DockerClient {
    /// Runs an action. Returns the bytes freed, for clean-ups.
    pub async fn perform(&self, action: Action, id: &str) -> Result<Option<i64>, String> {
        if let Action::Prune(kind) = action {
            return self.prune(kind).await.map(Some);
        }
        let docker = &self.docker;
        let result = match action {
            Action::Start => {
                docker
                    .start_container(id, None::<StartContainerOptions>)
                    .await
            }
            Action::Stop => {
                docker
                    .stop_container(id, None::<StopContainerOptions>)
                    .await
            }
            Action::Restart => {
                docker
                    .restart_container(id, None::<RestartContainerOptions>)
                    .await
            }
            Action::Pause => docker.pause_container(id).await,
            Action::Unpause => docker.unpause_container(id).await,
            Action::Remove(Resource::Containers) => {
                docker
                    .remove_container(id, None::<RemoveContainerOptions>)
                    .await
            }
            Action::Remove(Resource::Images) => {
                // `force` lets an image with several tags go in one step. The
                // UI only offers this for images no container uses.
                let opts = RemoveImageOptions {
                    force: true,
                    ..Default::default()
                };
                docker.remove_image(id, Some(opts), None).await.map(drop)
            }
            Action::Remove(Resource::Volumes) => {
                docker.remove_volume(id, None::<RemoveVolumeOptions>).await
            }
            Action::Remove(Resource::Networks) => docker.remove_network(id).await,
            Action::Remove(Resource::Disk)
            | Action::Prune(_)
            | Action::Pull
            | Action::Run
            | Action::Create(_)
            | Action::Connect
            | Action::Disconnect
            | Action::Rename
            | Action::Update => {
                return Err("not something that can be removed".into());
            }
        };
        result.map(|()| None).map_err(describe)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remove_refreshes_its_own_list() {
        assert_eq!(Action::Stop.affects(), Resource::Containers);
        assert_eq!(
            Action::Remove(Resource::Volumes).affects(),
            Resource::Volumes
        );
        assert_eq!(Action::Remove(Resource::Images).past_tense(), "Removed");
    }
}
