//! Everything Ferrodock currently knows about the Docker Engine.
//!
//! The store is fed by worker responses and read by the pages. It holds no
//! UI state of its own.

mod live;

use std::collections::HashMap;

use crate::docker::{
    Action, ComposeProject, ContainerInfo, DiskUsage, ImageInfo, Inspection, NetworkInfo,
    PullProgress, VolumeInfo, format_size, group_projects,
};
use crate::worker::Response;
pub use live::Live;

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
    /// Compose projects, derived from `containers` whenever they change.
    pub projects: Vec<ComposeProject>,
    pub images: Vec<ImageInfo>,
    pub volumes: Vec<VolumeInfo>,
    pub networks: Vec<NetworkInfo>,
    /// Disk usage, once the Disk page has loaded it.
    pub disk: Option<DiskUsage>,
    /// The item whose inspect output was asked for last, and the result
    /// once it arrives (an error message if it failed).
    pub inspecting: Option<String>,
    pub inspection: Option<Result<Inspection, String>>,
    /// Pulls in progress, by image reference.
    pub pulls: Vec<(String, PullProgress)>,
    /// IDs (names, for volumes) of items with an action in flight, and
    /// the action.
    pub pending: HashMap<String, Action>,
    /// Logs and stats of the container being watched, if any.
    pub live: Option<Live>,
}

impl Store {
    pub fn new() -> Self {
        Self {
            connection: Connection::Connecting,
            containers: Vec::new(),
            projects: Vec::new(),
            images: Vec::new(),
            volumes: Vec::new(),
            networks: Vec::new(),
            disk: None,
            inspecting: None,
            inspection: None,
            pulls: Vec::new(),
            pending: HashMap::new(),
            live: None,
        }
    }

    /// Applies a worker response. Returns a notice when the user should be
    /// told about something.
    pub fn apply(&mut self, response: Response) -> Option<Notice> {
        match response {
            Response::Connected { version } => self.connection = Connection::Connected { version },
            Response::Containers(list) => {
                self.projects = group_projects(&list);
                self.containers = list;
            }
            Response::Images(list) => self.images = list,
            Response::Volumes(list) => self.volumes = list,
            Response::Networks(list) => self.networks = list,
            Response::Disk(usage) => self.disk = Some(usage),
            Response::Alert(text) => {
                return Some(Notice {
                    text,
                    is_error: true,
                });
            }
            Response::Pulling { image, progress } => {
                match self.pulls.iter_mut().find(|(i, _)| *i == image) {
                    Some(entry) => entry.1 = progress,
                    None => self.pulls.push((image, progress)),
                }
            }
            Response::Inspected(inspection) => {
                if self.inspecting.as_deref() == Some(inspection.id.as_str()) {
                    self.inspection = Some(Ok(inspection));
                }
            }
            Response::InspectFailed { id, message } => {
                if self.inspecting.as_deref() == Some(id.as_str()) {
                    self.inspection = Some(Err(message));
                }
            }
            Response::Logs { container_id, text } => {
                if let Some(live) = self.live_for(&container_id) {
                    live.logs.push(&text);
                }
            }
            Response::Stats {
                container_id,
                sample,
            } => {
                if let Some(live) = self.live_for(&container_id) {
                    live.push_stats(sample);
                }
            }
            Response::Disconnected(reason) => {
                *self = Self::new();
                self.connection = Connection::Disconnected { reason };
            }
            Response::Done { task, freed } => {
                self.pending.remove(&task.id);
                self.pulls.retain(|(image, _)| *image != task.id);
                let mut text = format!("{} {}", task.action.past_tense(), task.name);
                if let Some(bytes) = freed.filter(|b| *b > 0) {
                    text.push_str(&format!(" · freed {}", format_size(bytes)));
                }
                return Some(Notice {
                    text,
                    is_error: false,
                });
            }
            Response::Failed { task, message } => {
                self.pending.remove(&task.id);
                self.pulls.retain(|(image, _)| *image != task.id);
                return Some(Notice {
                    text: message,
                    is_error: true,
                });
            }
        }
        None
    }

    /// The live data, if it belongs to `container_id`. Late messages from a
    /// container that's no longer watched are dropped.
    fn live_for(&mut self, container_id: &str) -> Option<&mut Live> {
        self.live
            .as_mut()
            .filter(|live| live.container_id == container_id)
    }

    pub fn is_disconnected(&self) -> bool {
        matches!(self.connection, Connection::Disconnected { .. })
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
    use crate::docker::{Action, Resource, Task};

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
            compose: None,
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

    fn task(action: Action, id: &str) -> Task {
        Task {
            action,
            id: id.into(),
            name: "web".into(),
        }
    }

    #[test]
    fn action_results_clear_pending_and_notify() {
        let mut store = Store::new();
        store.pending.insert("x".into(), Action::Start);
        let notice = store
            .apply(Response::Failed {
                task: task(Action::Start, "x"),
                message: "boom".into(),
            })
            .unwrap();
        assert!(notice.is_error);
        assert!(store.pending.is_empty());

        let notice = store
            .apply(Response::Done {
                task: task(Action::Remove(Resource::Volumes), "y"),
                freed: None,
            })
            .unwrap();
        assert_eq!(notice.text, "Removed web");
    }

    #[test]
    fn disconnect_forgets_everything() {
        let mut store = Store::new();
        store.apply(Response::Containers(vec![container("web", "", &[], &[])]));
        store.pending.insert("web-id".into(), Action::Stop);
        store.apply(Response::Disconnected("gone".into()));
        assert!(store.containers.is_empty());
        assert!(store.pending.is_empty());
        assert!(matches!(store.connection, Connection::Disconnected { .. }));
    }
}
