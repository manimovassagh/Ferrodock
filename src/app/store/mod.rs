//! Everything Ferrodock currently knows about the Docker Engine.
//!
//! The store is fed by worker responses and read by the pages. It holds no
//! UI state of its own.

mod compose;
mod files;
mod live;
mod terminal;

pub use compose::{ComposeJobs, Job};
pub use files::FileView;
pub use terminal::Terminal;

use std::collections::{HashMap, VecDeque};
use std::time::Instant;

use crate::docker::{
    Action, ComposeProject, ContainerInfo, DiskUsage, Engine, ImageInfo, Inspection, Layer,
    NetworkInfo, Processes, PullProgress, SearchResult, StatsSample, VolumeInfo, format_size,
    group_projects,
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

/// User-facing result of a finished action, or news nobody asked for.
#[derive(Default)]
pub struct Notice {
    pub text: String,
    pub is_error: bool,
    /// Something went wrong on its own (a container crashed): worth a
    /// system notification when the window isn't in front.
    pub alert: bool,
}

/// Docker Hub's results, or why there are none.
pub type SearchAnswer = Result<Vec<SearchResult>, String>;

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
    /// The volume being browsed, and its helper container once made.
    pub volume_browser: Option<(String, Option<Result<String, String>>)>,
    /// The open shell, if any.
    pub terminal: Option<Terminal>,
    /// `docker compose` jobs: the last command, and followed logs.
    pub compose: ComposeJobs,
    /// Compose files brought up from Ferrodock, newest first (a copy of
    /// the setting, for the Compose page).
    pub recent_compose: Vec<String>,
    /// Engines to choose from, and the name of the one in use.
    pub engines: Vec<Engine>,
    pub engine: Option<String>,
    /// The last Docker Hub search, and its results once in.
    pub search: Option<(String, Option<SearchAnswer>)>,
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
    /// Registry logins, for Settings.
    pub registries: crate::app::registries::Registries,
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
            terminal: None,
            search: None,
            compose: ComposeJobs::default(),
            recent_compose: Vec::new(),
            engines: Vec::new(),
            engine: None,
            volume_browser: None,
            pending: HashMap::new(),
            live: None,
            registries: Default::default(),
        }
    }

    /// Records that a file operation was asked for, so the Files tab shows
    /// it loading.
    pub fn files_requested(&mut self, op: &crate::docker::FileOp) {
        use crate::docker::FileOp;
        match op {
            FileOp::BrowseVolume { volume } => self.volume_browser = Some((volume.clone(), None)),
            FileOp::EndBrowse { volume } => {
                if self
                    .volume_browser
                    .as_ref()
                    .is_some_and(|(v, _)| v == volume)
                {
                    self.volume_browser = None;
                }
            }
            _ => files::requested(&mut self.files, op),
        }
    }

    /// Records a terminal request. Returns a session that's now replaced
    /// and should be closed.
    pub fn terminal_requested(&mut self, op: &crate::docker::TermOp) -> Option<u64> {
        terminal::requested(&mut self.terminal, op)
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
                    alert: true,
                });
            }
            Response::Pulling { image, progress } => {
                match self.pulls.iter_mut().find(|(i, _)| *i == image) {
                    Some(entry) => entry.1 = progress,
                    None => self.pulls.push((image, progress)),
                }
            }
            Response::ComposeOutput { job, lines } => self.compose.output(job, lines),
            Response::ComposeDone { job, error } => {
                if let Some((text, is_error)) = self.compose.done(job, error) {
                    return Some(Notice {
                        text,
                        is_error,
                        ..Default::default()
                    });
                }
            }
            Response::SearchResults { term, results } => {
                if let Some((asked, slot)) = &mut self.search
                    && *asked == term
                {
                    *slot = Some(results);
                }
            }
            Response::Registries { logins, outcome } => self.registries.answer(logins, outcome),
            Response::VolumeHelper { volume, result } => {
                if let Some((asked, slot)) = &mut self.volume_browser
                    && *asked == volume
                {
                    *slot = Some(result);
                }
            }
            Response::TermOutput { session, bytes } => {
                terminal::output(&mut self.terminal, session, &bytes);
            }
            Response::TermExit {
                session,
                message,
                no_shell,
            } => {
                terminal::exited(&mut self.terminal, session, message, no_shell);
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
                return Some(Notice {
                    text,
                    is_error,
                    ..Default::default()
                });
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
            // The app handles these itself.
            Response::EngineStarted(_) | Response::Provider(_) => {}
            Response::Disconnected(reason) => {
                // Which engines there are doesn't depend on reaching one,
                // and switching is how to get out of a dead engine.
                let (engines, engine) = (std::mem::take(&mut self.engines), self.engine.take());
                *self = Self::new();
                self.engines = engines;
                self.engine = engine;
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
                    ..Default::default()
                });
            }
            Response::Failed { task, message } => {
                self.pending.remove(&task.id);
                self.pulls.retain(|(image, _)| *image != task.id);
                return Some(Notice {
                    text: message,
                    is_error: true,
                    ..Default::default()
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
        store.engine = Some("work".into());
        store.apply(Response::Disconnected("gone".into()));
        assert_eq!(store.engine.as_deref(), Some("work"), "the engine is kept");
        assert!(store.containers.is_empty());
        assert!(store.pending.is_empty());
        assert!(matches!(store.connection, Connection::Disconnected { .. }));
    }

    #[test]
    fn crashes_are_alerts_and_results_are_not() {
        let mut store = Store::new();
        let alert = store.apply(Response::Alert("web crashed".into())).unwrap();
        assert!(alert.alert && alert.is_error);
        let done = store
            .apply(Response::Done {
                task: task(Action::Stop, "x"),
                freed: None,
            })
            .unwrap();
        assert!(!done.alert);
    }
}
