//! Things Ferrodock can do to a single item: container lifecycle and
//! removing any resource.

use bollard::query_parameters::{
    RemoveContainerOptions, RemoveImageOptions, RemoveVolumeOptions, RestartContainerOptions,
    StartContainerOptions, StopContainerOptions,
};

use super::{DockerClient, Resource, describe};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Start,
    Stop,
    Restart,
    Pause,
    Unpause,
    Remove(Resource),
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
            Action::Remove(_) => "Removed",
        }
    }

    /// The list that changes when this action runs.
    pub fn affects(self) -> Resource {
        match self {
            Action::Remove(resource) => resource,
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
    pub async fn perform(&self, action: Action, id: &str) -> Result<(), String> {
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
        };
        result.map_err(describe)
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
