//! The Engine's event stream (`docker events`): what changed, as it happens.

use std::collections::HashMap;

use bollard::models::{EventMessage, EventMessageTypeEnum};
use bollard::query_parameters::EventsOptions;
use futures_util::{Stream, StreamExt};

use super::{DockerClient, Resource, describe};

/// One thing that happened in the Engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineEvent {
    /// The list that changed.
    pub resource: Resource,
    /// e.g. `start`, `die`, `oom`, `destroy`.
    pub action: String,
    pub id: String,
    /// The container's name, for containers.
    pub name: String,
    /// The exit code, for `die` events.
    pub exit_code: Option<i64>,
}

impl EngineEvent {
    fn from_message(m: EventMessage) -> Option<Self> {
        let resource = match m.typ? {
            EventMessageTypeEnum::CONTAINER => Resource::Containers,
            EventMessageTypeEnum::IMAGE => Resource::Images,
            EventMessageTypeEnum::VOLUME => Resource::Volumes,
            EventMessageTypeEnum::NETWORK => Resource::Networks,
            _ => return None,
        };
        let actor = m.actor.unwrap_or_default();
        let attributes = actor.attributes.unwrap_or_default();
        // Exec and health-check chatter ("exec_start: …", "health_status: …")
        // doesn't change any list.
        let action = m.action.unwrap_or_default();
        let action = action.split(':').next().unwrap_or_default().to_owned();
        if action.starts_with("exec_") || action == "health_status" || action == "top" {
            return None;
        }
        Some(Self {
            resource,
            exit_code: attributes.get("exitCode").and_then(|c| c.parse().ok()),
            name: attributes.get("name").cloned().unwrap_or_default(),
            id: actor.id.unwrap_or_default(),
            action,
        })
    }
}

impl DockerClient {
    /// Follows the Engine's events for containers, images, volumes and
    /// networks until the connection drops.
    pub fn events(&self) -> impl Stream<Item = Result<EngineEvent, String>> + use<> {
        let filters = HashMap::from([(
            "type".to_owned(),
            ["container", "image", "volume", "network"]
                .map(str::to_owned)
                .to_vec(),
        )]);
        let options = EventsOptions {
            filters: Some(filters),
            ..Default::default()
        };
        self.docker
            .events(Some(options))
            .filter_map(|item| async move {
                match item {
                    Ok(message) => EngineEvent::from_message(message).map(Ok),
                    Err(e) => Some(Err(describe(e))),
                }
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bollard::models::EventActor;

    fn message(typ: EventMessageTypeEnum, action: &str, attrs: &[(&str, &str)]) -> EventMessage {
        EventMessage {
            typ: Some(typ),
            action: Some(action.into()),
            actor: Some(EventActor {
                id: Some("abc".into()),
                attributes: Some(
                    attrs
                        .iter()
                        .map(|(k, v)| (k.to_string(), v.to_string()))
                        .collect(),
                ),
            }),
            ..Default::default()
        }
    }

    #[test]
    fn reads_container_deaths() {
        let e = EngineEvent::from_message(message(
            EventMessageTypeEnum::CONTAINER,
            "die",
            &[("name", "api"), ("exitCode", "137")],
        ))
        .unwrap();
        assert_eq!(e.resource, Resource::Containers);
        assert_eq!(e.name, "api");
        assert_eq!(e.exit_code, Some(137));
    }

    #[test]
    fn ignores_exec_and_health_chatter() {
        for action in ["exec_start: sh", "health_status: healthy", "exec_die"] {
            let m = message(EventMessageTypeEnum::CONTAINER, action, &[]);
            assert!(EngineEvent::from_message(m).is_none(), "{action}");
        }
        let m = message(EventMessageTypeEnum::DAEMON, "reload", &[]);
        assert!(EngineEvent::from_message(m).is_none());
    }

    /// Talks to a real daemon: `cargo test -- --ignored`. Starts and removes
    /// a container and expects to hear about it.
    #[tokio::test]
    #[ignore]
    async fn live_daemon_reports_a_start() {
        use crate::RunSpec;
        use bollard::query_parameters::RemoveContainerOptions;
        use std::time::Duration;

        let client = DockerClient::connect().unwrap();
        let Some(image) = client.list_images().await.unwrap().into_iter().next() else {
            return;
        };
        // Listen first: the stream only connects once it's polled.
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let listener = {
            let client = client.clone();
            tokio::spawn(async move {
                let mut events = Box::pin(client.events());
                while let Some(Ok(event)) = events.next().await {
                    let _ = tx.send(event);
                }
            })
        };
        tokio::time::sleep(Duration::from_millis(500)).await;
        let spec = RunSpec {
            image: image.id,
            name: "ferrodock-events-test".into(),
            ..Default::default()
        };
        let id = client.run(&spec).await.unwrap();
        let started = tokio::time::timeout(Duration::from_secs(10), async {
            while let Some(event) = rx.recv().await {
                if event.id == id && event.action == "start" {
                    return true;
                }
            }
            false
        })
        .await
        .unwrap_or(false);
        listener.abort();
        let opts = RemoveContainerOptions {
            force: true,
            ..Default::default()
        };
        client
            .docker
            .remove_container(&id, Some(opts))
            .await
            .unwrap();
        assert!(started);
    }
}
