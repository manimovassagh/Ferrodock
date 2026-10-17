//! Background worker that owns the async runtime and the Docker client.
//!
//! The UI thread never blocks: it sends [`Request`]s and drains
//! [`Response`]s once per frame.

mod dedup;
mod live;

use std::sync::{Arc, mpsc};
use std::thread;
use std::time::Duration;

use tokio::sync::mpsc as tokio_mpsc;

use crate::docker::{
    Action, ContainerInfo, DiskUsage, DockerClient, ImageInfo, Inspection, NetworkInfo, Resource,
    StatsSample, Task, VolumeInfo,
};

/// How often the worker re-checks the engine on its own.
const POLL_INTERVAL: Duration = Duration::from_secs(3);

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
    Inspected(Inspection),
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
}

async fn run(mut requests: tokio_mpsc::UnboundedReceiver<Request>, out: Sender) {
    let shared = Arc::new(Shared {
        out,
        dedup: dedup::Dedup::default(),
    });
    let mut client: Option<DockerClient> = None;
    let mut watch = live::Watch::default();
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
        client = Some(docker.clone());

        let shared = shared.clone();
        match request {
            Request::Watch(Some(container_id)) => {
                watch.start(&docker, &shared, container_id);
            }
            // Each request runs as its own task so a slow `stop` (which
            // waits for the container to exit) doesn't hold up refreshes.
            Request::Refresh(resource) | Request::Poll(resource) => {
                tokio::spawn(
                    async move { refresh_with_containers(&docker, &shared, resource).await },
                );
            }
            Request::Run(task) => {
                tokio::spawn(async move { run_task(&docker, &shared, task).await });
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
}

impl Shared {
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
