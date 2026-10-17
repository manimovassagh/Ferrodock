//! Background worker that owns the async runtime and the Docker client.
//!
//! The UI thread never blocks: it sends [`Request`]s and drains
//! [`Response`]s once per frame.

mod dedup;
mod files;
mod images;
mod live;
mod terminal;

use std::sync::{Arc, mpsc};
use std::thread;
use std::time::Duration;

use tokio::sync::mpsc as tokio_mpsc;

use crate::docker::{
    Action, Change, ContainerInfo, DiskUsage, DockerClient, EngineEvent, FileOp, ImageInfo,
    ImageOp, Inspection, Layer, Listing, NetworkInfo, Preview, Processes, PullProgress, Resource,
    RunSpec, SearchResult, StatsSample, Task, TermOp, VolumeInfo,
};

/// How often the worker re-checks the engine on its own. Changes normally
/// arrive at once through the event stream; this is the safety net.
const POLL_INTERVAL: Duration = Duration::from_secs(5);

/// Usage of every running container is read at most this often, and only
/// while the Containers page is open.
const USAGE_INTERVAL: Duration = Duration::from_secs(4);

/// Events arriving this close together are handled as one batch.
const EVENT_BATCH: Duration = Duration::from_millis(200);

#[derive(Debug)]
pub enum Request {
    /// Reload one list now.
    Refresh(Resource),
    /// Keep this list fresh, re-checking it (and the containers) every few
    /// seconds. Changed results are sent; unchanged ones are not.
    Poll(Resource),
    /// Run an action, then reload the list it changed.
    Run(Task),
    /// Stream logs and stats of this container (replacing any previous
    /// one), or stop streaming with `None`.
    Watch(Option<String>),
    /// Load the full inspect output of one item.
    Inspect(Resource, String),
    /// Pull an image, reporting progress as it goes.
    Pull(String),
    /// Create a volume or network, or (dis)connect a container.
    Change(Change),
    /// List the processes running in a container.
    Processes(String),
    /// Load the layers of an image.
    History(String),
    /// Create and start a container.
    RunImage(RunSpec),
    /// List, preview or copy files of a container.
    Files(FileOp),
    /// Open, type into, resize or close a shell.
    Terminal(TermOp),
    /// Search, tag, push, save or load images.
    Image(ImageOp),
}

#[derive(Debug)]
pub enum Response {
    Connected {
        version: String,
    },
    Containers(Vec<ContainerInfo>),
    Images(Vec<ImageInfo>),
    Volumes(Vec<VolumeInfo>),
    Networks(Vec<NetworkInfo>),
    Disk(DiskUsage),
    /// CPU and memory of every running container, by ID.
    Usage(Vec<(String, StatsSample)>),
    /// Something the user should know about even though they didn't ask,
    /// e.g. a container crashed.
    Alert(String),
    Inspected(Inspection),
    Processes(Result<Processes, (String, String)>),
    History {
        image: String,
        layers: Vec<Layer>,
    },
    /// A folder listing, or (container, path, why) it couldn't be read.
    Files(Result<Listing, (String, String, String)>),
    FilePreview {
        container: String,
        path: String,
        preview: Result<Preview, String>,
    },
    /// Docker Hub's answer to a search.
    SearchResults {
        term: String,
        results: Result<Vec<SearchResult>, String>,
    },
    /// The helper container for browsing a volume, or why there isn't one.
    VolumeHelper {
        volume: String,
        result: Result<String, String>,
    },
    /// What a shell printed.
    TermOutput {
        session: u64,
        bytes: Vec<u8>,
    },
    /// A shell ended; `message` says why when it wasn't a normal exit.
    TermExit {
        session: u64,
        message: Option<String>,
        /// The container has no shell; a debug shell would work.
        no_shell: bool,
    },
    /// A download or upload finished. `changed_dir` is the folder an upload
    /// changed, to list again.
    Transferred {
        container: String,
        changed_dir: Option<String>,
        text: String,
        is_error: bool,
    },
    /// Progress of a pull started with [`Request::Pull`].
    Pulling {
        image: String,
        progress: PullProgress,
    },
    InspectFailed {
        id: String,
        message: String,
    },
    /// The daemon could not be reached or listing failed.
    Disconnected(String),
    Done {
        task: Task,
        /// Bytes freed, for clean-ups.
        freed: Option<i64>,
    },
    Failed {
        task: Task,
        message: String,
    },
    /// New log output from the watched container.
    Logs {
        container_id: String,
        text: String,
    },
    /// A resource usage reading from the watched container.
    Stats {
        container_id: String,
        sample: StatsSample,
    },
}

pub struct Worker {
    requests: tokio_mpsc::UnboundedSender<Request>,
    responses: mpsc::Receiver<Response>,
}

impl Worker {
    /// Spawns the worker thread. `on_response` is called after each message
    /// is queued so the UI can wake up and repaint.
    pub fn spawn(on_response: impl Fn() + Send + Sync + 'static) -> Self {
        let (req_tx, req_rx) = tokio_mpsc::unbounded_channel();
        let (resp_tx, resp_rx) = mpsc::channel();

        thread::Builder::new()
            .name("docker-worker".into())
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(2)
                    .enable_all()
                    .build()
                    .expect("failed to build tokio runtime");
                runtime.block_on(run(
                    req_rx,
                    Sender {
                        tx: resp_tx,
                        notify: Box::new(on_response),
                    },
                ));
            })
            .expect("failed to spawn worker thread");

        Self {
            requests: req_tx,
            responses: resp_rx,
        }
    }

    pub fn send(&self, request: Request) {
        // Only fails if the worker thread died; nothing useful to do then.
        let _ = self.requests.send(request);
    }

    pub fn try_recv(&self) -> Option<Response> {
        self.responses.try_recv().ok()
    }
}

struct Sender {
    tx: mpsc::Sender<Response>,
    notify: Box<dyn Fn() + Send + Sync>,
}

impl Sender {
    fn send(&self, response: Response) {
        if self.tx.send(response).is_ok() {
            (self.notify)();
        }
    }
}

/// State shared by the worker's tasks.
struct Shared {
    out: Sender,
    dedup: dedup::Dedup,
    /// When usage of all containers was last read.
    usage_at: std::sync::Mutex<Option<std::time::Instant>>,
}

async fn run(mut requests: tokio_mpsc::UnboundedReceiver<Request>, out: Sender) {
    let shared = Arc::new(Shared {
        out,
        dedup: dedup::Dedup::default(),
        usage_at: std::sync::Mutex::new(None),
    });
    let mut client: Option<DockerClient> = None;
    let mut watch = live::Watch::default();
    let mut terminals = terminal::Sessions::default();
    let mut events: Option<tokio::task::JoinHandle<()>> = None;
    let mut polled = Resource::Containers;
    let mut ticker = tokio::time::interval(POLL_INTERVAL);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        let request = tokio::select! {
            request = requests.recv() => match request {
                Some(request) => request,
                None => break, // The UI is gone.
            },
            // Disk usage is slow to compute, so only its containers are polled.
            _ = ticker.tick() => Request::Refresh(match polled {
                Resource::Disk => Resource::Containers,
                other => other,
            }),
        };

        // Stopping a watch must work even while Docker is unreachable.
        match request {
            Request::Watch(None) => {
                watch.stop();
                continue;
            }
            Request::Poll(resource) => {
                polled = resource;
                ticker.reset();
            }
            _ => {}
        }

        // Docker may not be running yet (or its socket may not exist), so
        // keep trying to set up the client until it works.
        let docker = match client.clone().map_or_else(DockerClient::connect, Ok) {
            Ok(docker) => docker,
            Err(e) => {
                shared.disconnected(e);
                continue;
            }
        };
        if client.is_none() {
            // Helpers left from browsing volumes in an earlier run.
            let docker = docker.clone();
            tokio::spawn(async move {
                let _ = docker.remove_helpers(None).await;
                let _ = docker.remove_debug_helpers().await;
            });
        }
        client = Some(docker.clone());
        // (Re)start following the event stream; it ends when the
        // connection drops.
        if events.as_ref().is_none_or(|task| task.is_finished()) {
            events = Some(tokio::spawn(follow_events(docker.clone(), shared.clone())));
        }

        let shared = shared.clone();
        match request {
            Request::Watch(Some(container_id)) => {
                watch.start(&docker, &shared, container_id);
            }
            // Each request runs as its own task so a slow `stop` (which
            // waits for the container to exit) doesn't hold up refreshes.
            Request::Refresh(resource) | Request::Poll(resource) => {
                if polled == Resource::Containers && shared.usage_due() {
                    let (docker, shared) = (docker.clone(), shared.clone());
                    tokio::spawn(async move { usage(&docker, &shared).await });
                }
                tokio::spawn(
                    async move { refresh_with_containers(&docker, &shared, resource).await },
                );
            }
            Request::Run(task) => {
                tokio::spawn(async move { run_task(&docker, &shared, task).await });
            }
            Request::Change(change) => {
                tokio::spawn(async move {
                    let task = change.task();
                    let affected = task.action.affects();
                    match docker.apply(&change).await {
                        Ok(()) => shared.send(Response::Done { task, freed: None }),
                        Err(e) => {
                            let message =
                                format!("Couldn't {} {}: {e}", task.action.verb(), task.name);
                            shared.send(Response::Failed { task, message });
                        }
                    }
                    refresh(&docker, &shared, affected).await;
                    refresh(&docker, &shared, Resource::Containers).await;
                });
            }
            Request::Processes(id) => {
                tokio::spawn(async move {
                    let result = docker.processes(&id).await.map_err(|e| (id, e));
                    shared.send(Response::Processes(result));
                });
            }
            Request::History(image) => {
                tokio::spawn(async move {
                    // A failure just leaves the history empty.
                    let layers = docker.history(&image).await.unwrap_or_default();
                    shared.send(Response::History { image, layers });
                });
            }
            Request::Pull(image) => {
                tokio::spawn(async move { pull(&docker, &shared, image).await });
            }
            Request::Terminal(op) => terminals.handle(&docker, &shared, op),
            Request::Image(op) => {
                tokio::spawn(async move { images::handle(&docker, &shared, op).await });
            }
            Request::Files(op) => {
                tokio::spawn(async move { files::handle(&docker, &shared, op).await });
            }
            Request::RunImage(spec) => {
                tokio::spawn(async move { run_image(&docker, &shared, spec).await });
            }
            Request::Inspect(resource, id) => {
                tokio::spawn(async move {
                    let response = match docker.inspect(resource, &id).await {
                        Ok(inspection) => Response::Inspected(inspection),
                        Err(message) => Response::InspectFailed { id, message },
                    };
                    shared.send(response);
                });
            }
            Request::Watch(None) => unreachable!("handled above"),
        }
    }
    watch.stop();
    if let Some(task) = events {
        task.abort();
    }
}

/// Refreshes whatever the Engine reports changing, in batches, and warns
/// about containers that crash or run out of memory.
async fn follow_events(client: DockerClient, shared: Arc<Shared>) {
    use futures_util::StreamExt;
    use std::collections::{HashMap, HashSet};
    use tokio::time::Instant;

    let mut stream = Box::pin(client.events());
    // Containers being stopped on purpose (by anyone), so their exit isn't
    // reported as a crash.
    let mut stopping: HashMap<String, Instant> = HashMap::new();
    while let Some(Ok(first)) = stream.next().await {
        let mut changed = HashSet::new();
        let mut note = |event: EngineEvent, changed: &mut HashSet<Resource>| {
            changed.insert(event.resource);
            if event.resource != Resource::Containers {
                return;
            }
            match event.action.as_str() {
                "kill" | "stop" | "pause" => {
                    stopping.insert(event.id, Instant::now());
                }
                "oom" => shared.send(Response::Alert(format!("{} ran out of memory", event.name))),
                "die" => {
                    let on_purpose = stopping
                        .remove(&event.id)
                        .is_some_and(|at| at.elapsed() < Duration::from_secs(30));
                    if let Some(code) = event.exit_code.filter(|c| *c != 0 && !on_purpose) {
                        shared.send(Response::Alert(format!(
                            "{} stopped unexpectedly (exit code {code})",
                            event.name
                        )));
                    }
                }
                _ => {}
            }
        };
        note(first, &mut changed);
        let deadline = Instant::now() + EVENT_BATCH;
        while let Ok(Some(Ok(event))) = tokio::time::timeout_at(deadline, stream.next()).await {
            note(event, &mut changed);
        }
        // Containers carry the "in use" info every page shows.
        changed.insert(Resource::Containers);
        for resource in changed {
            refresh(&client, &shared, resource).await;
        }
    }
}

impl Shared {
    /// Whether it's time to read usage again; claims the slot if so.
    fn usage_due(&self) -> bool {
        let Ok(mut at) = self.usage_at.lock() else {
            return false;
        };
        if at.is_some_and(|t| t.elapsed() < USAGE_INTERVAL) {
            return false;
        }
        *at = Some(std::time::Instant::now());
        true
    }

    fn send(&self, response: Response) {
        self.out.send(response);
    }

    /// Reports a lost connection. Everything is sent again once it's back,
    /// because the UI clears its data on disconnect.
    fn disconnected(&self, reason: String) {
        self.dedup.reset();
        self.send(Response::Disconnected(reason));
    }
}

/// Pulls an image, sending progress at most ten times a second.
async fn pull(client: &DockerClient, shared: &Shared, image: String) {
    let task = Task {
        action: Action::Pull,
        id: image.clone(),
        name: image.clone(),
    };
    let mut last = std::time::Instant::now() - Duration::from_secs(1);
    let result = client
        .pull(&image, |progress| {
            if last.elapsed() >= Duration::from_millis(100) {
                last = std::time::Instant::now();
                shared.send(Response::Pulling {
                    image: image.clone(),
                    progress,
                });
            }
        })
        .await;
    match result {
        Ok(()) => shared.send(Response::Done { task, freed: None }),
        Err(e) => {
            let message = format!("Couldn't pull {image}: {e}");
            shared.send(Response::Failed { task, message });
        }
    }
    refresh(client, shared, Resource::Images).await;
}

async fn run_image(client: &DockerClient, shared: &Shared, spec: RunSpec) {
    let label = if spec.name.trim().is_empty() {
        spec.image.clone()
    } else {
        spec.name.trim().to_owned()
    };
    let task = Task {
        action: Action::Run,
        id: format!("run:{}", spec.image),
        name: label,
    };
    match client.run(&spec).await {
        Ok(_) => shared.send(Response::Done { task, freed: None }),
        Err(e) => {
            let message = format!("Couldn't start {}: {e}", task.name);
            shared.send(Response::Failed { task, message });
        }
    }
    refresh(client, shared, Resource::Containers).await;
}

/// Reads CPU and memory of every running container, all at once.
async fn usage(client: &DockerClient, shared: &Shared) {
    let Ok(containers) = client.list_containers().await else {
        return;
    };
    let readings = containers
        .iter()
        .filter(|c| c.is_running())
        .map(|c| async move { (c.id.clone(), client.stats_once(&c.id).await) });
    let readings = futures_util::future::join_all(readings).await;
    let usage = readings
        .into_iter()
        .filter_map(|(id, reading)| reading.ok().map(|r| (id, r)))
        .collect();
    shared.send(Response::Usage(usage));
}

async fn run_task(client: &DockerClient, shared: &Shared, task: Task) {
    match client.perform(task.action, &task.id).await {
        Ok(freed) => {
            let affected = task.action.affects();
            let pruned = matches!(task.action, Action::Prune(_));
            shared.send(Response::Done { task, freed });
            if affected != Resource::Containers {
                refresh(client, shared, affected).await;
            }
            if pruned && affected != Resource::Disk {
                refresh(client, shared, Resource::Disk).await;
            }
        }
        Err(e) => {
            let message = format!("Couldn't {} {}: {e}", task.action.verb(), task.name);
            shared.send(Response::Failed { task, message });
        }
    }
    // Container state may change either way (and removals change the
    // "in use" info), so always reload them.
    refresh(client, shared, Resource::Containers).await;
}

/// Containers drive the connection status and the "in use" info on every
/// page, so they're refreshed alongside any other list.
async fn refresh_with_containers(client: &DockerClient, shared: &Shared, resource: Resource) {
    refresh(client, shared, Resource::Containers).await;
    if resource == Resource::Disk {
        // The clean-up preview lists these, so bring them up to date too.
        for list in [Resource::Images, Resource::Volumes, Resource::Networks] {
            refresh(client, shared, list).await;
        }
    }
    if resource != Resource::Containers {
        refresh(client, shared, resource).await;
    }
}

/// Loads one list and sends it if it changed since last time.
async fn refresh(client: &DockerClient, shared: &Shared, resource: Resource) {
    use dedup::Key;

    if resource == Resource::Containers {
        // The containers list doubles as the connection heartbeat.
        match client.version().await {
            Ok(version) => {
                if shared.dedup.changed(Key::Version, &version) {
                    shared.send(Response::Connected { version });
                }
            }
            Err(e) => return shared.disconnected(e),
        }
    }

    let key = Key::List(resource);
    let response = match resource {
        Resource::Containers => client.list_containers().await.map(|l| {
            shared
                .dedup
                .changed(key, &l)
                .then_some(Response::Containers(l))
        }),
        Resource::Images => client
            .list_images()
            .await
            .map(|l| shared.dedup.changed(key, &l).then_some(Response::Images(l))),
        Resource::Volumes => client.list_volumes().await.map(|l| {
            shared
                .dedup
                .changed(key, &l)
                .then_some(Response::Volumes(l))
        }),
        Resource::Networks => client.list_networks().await.map(|l| {
            shared
                .dedup
                .changed(key, &l)
                .then_some(Response::Networks(l))
        }),
        // Always sent: it's only loaded when asked for.
        Resource::Disk => client.disk_usage().await.map(|d| Some(Response::Disk(d))),
    };
    match response {
        Ok(Some(response)) => shared.send(response),
        Ok(None) => {}
        Err(e) => shared.disconnected(e),
    }
}
