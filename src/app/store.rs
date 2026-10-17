//! Everything IronStack currently knows about the Docker Engine.
//!
//! The store is fed by worker responses and read by the pages. It holds no
//! UI state of its own.

use std::collections::HashSet;

use crate::docker::{Action, ContainerInfo, ImageInfo, NetworkInfo, VolumeInfo};
use crate::worker::Response;

pub enum Connection {
    Connecting,
    Connected { version: String },
    Disconnected { reason: String },
}

/// User-facing result of a finished action.
pub struct Notice {
    pub text: String,
    pub is_error: bool,
}

pub struct Store {
    pub connection: Connection,
    pub containers: Vec<ContainerInfo>,
    pub images: Vec<ImageInfo>,
    pub volumes: Vec<VolumeInfo>,
    pub networks: Vec<NetworkInfo>,
    /// Container IDs with an action in flight.
    pub pending: HashSet<String>,
}

impl Store {
    pub fn new() -> Self {
        Self {
            connection: Connection::Connecting,
            containers: Vec::new(),
            images: Vec::new(),
            volumes: Vec::new(),
            networks: Vec::new(),
            pending: HashSet::new(),
        }
    }

    /// Applies a worker response. Returns a notice when the user should be
    /// told about something.
    pub fn apply(&mut self, response: Response) -> Option<Notice> {
        match response {
            Response::Connected { version } => self.connection = Connection::Connected { version },
            Response::Containers(list) => self.containers = list,
            Response::Images(list) => self.images = list,
            Response::Volumes(list) => self.volumes = list,
            Response::Networks(list) => self.networks = list,
            Response::Disconnected(reason) => {
                *self = Self::new();
                self.connection = Connection::Disconnected { reason };
            }
            Response::ActionDone { id, action, name } => {
                self.pending.remove(&id);
                let past = match action {
                    Action::Start => "Started",
                    Action::Stop => "Stopped",
                    Action::Restart => "Restarted",
                };
                return Some(Notice {
                    text: format!("{past} {name}"),
                    is_error: false,
                });
            }
            Response::ActionFailed { id, message } => {
                self.pending.remove(&id);
                return Some(Notice {
                    text: message,
                    is_error: true,
                });
            }
        }
        None
    }

    pub fn container(&self, id: &str) -> Option<&ContainerInfo> {
        self.containers.iter().find(|c| c.id == id)
    }

    pub fn running_count(&self) -> usize {
        self.containers.iter().filter(|c| c.is_active()).count()
    }

    /// Names of the containers created from an image.
    pub fn containers_using_image(&self, image_id: &str) -> Vec<&str> {
        self.container_names(|c| c.image_id == image_id)
    }

    /// Names of the containers that mount a volume.
    pub fn containers_using_volume(&self, volume: &str) -> Vec<&str> {
        self.container_names(|c| c.volumes.iter().any(|v| v == volume))
    }

    /// Names of the containers attached to a network.
    pub fn containers_on_network(&self, network: &str) -> Vec<&str> {
        self.container_names(|c| c.networks.iter().any(|n| n == network))
    }

    fn container_names(&self, pred: impl Fn(&ContainerInfo) -> bool) -> Vec<&str> {
        self.containers
            .iter()
            .filter(|c| pred(c))
            .map(|c| c.name.as_str())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn container(name: &str, image_id: &str, volumes: &[&str], networks: &[&str]) -> ContainerInfo {
        ContainerInfo {
            id: format!("{name}-id"),
            name: name.into(),
            image: String::new(),
            image_id: image_id.into(),
            command: String::new(),
            state: "running".into(),
            status: String::new(),
            created: 0,
            ports: Vec::new(),
            ip_addresses: Vec::new(),
            networks: networks.iter().map(|s| s.to_string()).collect(),
            mounts: Vec::new(),
            volumes: volumes.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn answers_used_by_questions() {
        let mut store = Store::new();
        store.apply(Response::Containers(vec![
            container("web", "sha256:aaa", &["data"], &["bridge"]),
            container("db", "sha256:bbb", &["data", "logs"], &["backend"]),
        ]));
        assert_eq!(store.containers_using_image("sha256:aaa"), vec!["web"]);
        assert_eq!(store.containers_using_volume("data"), vec!["web", "db"]);
        assert_eq!(store.containers_on_network("backend"), vec!["db"]);
        assert!(store.containers_using_volume("nope").is_empty());
        assert_eq!(store.running_count(), 2);
    }

    #[test]
    fn action_results_clear_pending_and_notify() {
        let mut store = Store::new();
        store.pending.insert("x".into());
        let notice = store
            .apply(Response::ActionFailed {
                id: "x".into(),
                message: "boom".into(),
            })
            .unwrap();
        assert!(notice.is_error);
        assert!(store.pending.is_empty());

        let notice = store
            .apply(Response::ActionDone {
                id: "y".into(),
                action: Action::Stop,
                name: "web".into(),
            })
            .unwrap();
        assert_eq!(notice.text, "Stopped web");
    }

    #[test]
    fn disconnect_forgets_everything() {
        let mut store = Store::new();
        store.apply(Response::Containers(vec![container("web", "", &[], &[])]));
        store.pending.insert("web-id".into());
        store.apply(Response::Disconnected("gone".into()));
        assert!(store.containers.is_empty());
        assert!(store.pending.is_empty());
        assert!(matches!(store.connection, Connection::Disconnected { .. }));
    }
}
