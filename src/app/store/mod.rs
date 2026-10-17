//! Everything Ferrodock currently knows about the Docker Engine.
//!
//! The store is fed by worker responses and read by the pages. It holds no
//! UI state of its own.

mod files;
mod live;

pub use files::FileView;

use std::collections::{HashMap, VecDeque};
use std::time::Instant;

use crate::docker::{
    Action, ComposeProject, ContainerInfo, DiskUsage, ImageInfo, Inspection, Layer, NetworkInfo,
    Processes, PullProgress, StatsSample, VolumeInfo, format_size, group_projects,
};
use crate::worker::Response;
pub use live::Live;

/// Usage readings kept per container for the table's sparklines.
const USAGE_HISTORY: usize = 20;

/// A container's processes, or why they couldn't be read.
pub type ProcessTable = Result<Processes, String>;

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
    /// The process table of the container asked about last, when it was
    /// asked for, and the answer (or why it couldn't be read).
    pub processes: Option<(String, Instant, Option<ProcessTable>)>,
    /// The layers of the image asked about last.
    pub history: Option<(String, Option<Vec<Layer>>)>,
    /// The Files tab of the container it was last opened for.
    pub files: Option<FileView>,
    /// Recent CPU and memory of each running container (oldest first),
    /// while the Containers page is open.
    pub usage: HashMap<String, VecDeque<StatsSample>>,
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
            usage: HashMap::new(),
            processes: None,
            history: None,
            files: None,
            pending: HashMap::new(),
            live: None,
        }
    }

    /// Records that a file operation was asked for, so the Files tab shows
    /// it loading.
    pub fn files_requested(&mut self, op: &crate::docker::FileOp) {
        files::requested(&mut self.files, op);
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
            Response::Usage(readings) => {
                // Forget containers that stopped.
                self.usage
                    .retain(|id, _| readings.iter().any(|(r, _)| r == id));
                for (id, sample) in readings {
                    let history = self.usage.entry(id).or_default();
                    if history.len() == USAGE_HISTORY {
                        history.pop_front();
                    }
                    history.push_back(sample);
                }
            }
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
            Response::Files(result) => files::listed(&mut self.files, result),
            Response::FilePreview {
                container,
                path,
                preview,
            } => files::previewed(&mut self.files, &container, &path, preview),
            Response::Transferred {
                container,
                changed_dir,
                text,
                is_error,
            } => {
                files::transferred(&mut self.files, &container, changed_dir.as_deref());
                return Some(Notice { text, is_error });
            }
            Response::Processes(result) => {
                let id = match &result {
                    Ok(p) => p.container_id.clone(),
                    Err((id, _)) => id.clone(),
                };
                if let Some((asked, _, slot)) = &mut self.processes
                    && *asked == id
                {
                    *slot = Some(result.map_err(|(_, e)| e));
                }
            }
            Response::History { image, layers } => {
                if let Some((asked, slot)) = &mut self.history
                    && *asked == image
                {
                    *slot = Some(layers);
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
