//! Background worker that owns the async runtime and the Docker client.
//!
//! The UI thread never blocks: it sends [`Request`]s and drains
//! [`Response`]s once per frame.

use std::sync::mpsc;
use std::thread;

use tokio::sync::mpsc as tokio_mpsc;

use crate::docker::{Action, ContainerInfo, DockerClient, ImageInfo, NetworkInfo, VolumeInfo};

/// Things the UI can ask to have (re)loaded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resource {
    Containers,
    Images,
    Volumes,
    Networks,
}

#[derive(Debug)]
pub enum Request {
    Refresh(Resource),
    Container {
        action: Action,
        id: String,
        name: String,
    },
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
    /// The daemon could not be reached or listing failed.
    Disconnected(String),
    ActionDone {
        id: String,
        action: Action,
        name: String,
    },
    ActionFailed {
        id: String,
        message: String,
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

async fn run(mut requests: tokio_mpsc::UnboundedReceiver<Request>, out: Sender) {
    let out = std::sync::Arc::new(out);
    let mut client: Option<DockerClient> = None;

    while let Some(request) = requests.recv().await {
        // Docker may not be running yet (or its socket may not exist), so
        // keep trying to set up the client until it works.
        let docker = match client.clone().map_or_else(DockerClient::connect, Ok) {
            Ok(docker) => docker,
            Err(e) => {
                out.send(Response::Disconnected(e));
                continue;
            }
        };
        client = Some(docker.clone());

        // Each request runs as its own task so a slow `stop` (which waits
        // for the container to exit) doesn't block refreshes.
        let client = docker;
        let out = out.clone();
        tokio::spawn(async move {
            match request {
                Request::Refresh(resource) => refresh(&client, &out, resource).await,
                Request::Container { action, id, name } => {
                    match client.perform(action, &id).await {
                        Ok(()) => out.send(Response::ActionDone { id, action, name }),
                        Err(e) => {
                            let message = format!("Failed to {} {name}: {e}", action.verb());
                            out.send(Response::ActionFailed { id, message })
                        }
                    }
                    refresh(&client, &out, Resource::Containers).await;
                }
            }
        });
    }
}

async fn refresh(client: &DockerClient, out: &Sender, resource: Resource) {
    let result = match resource {
        Resource::Containers => {
            // The containers list doubles as the connection heartbeat.
            match client.version().await {
                Ok(version) => out.send(Response::Connected { version }),
                Err(e) => return out.send(Response::Disconnected(e)),
            }
            client.list_containers().await.map(Response::Containers)
        }
        Resource::Images => client.list_images().await.map(Response::Images),
        Resource::Volumes => client.list_volumes().await.map(Response::Volumes),
        Resource::Networks => client.list_networks().await.map(Response::Networks),
    };
    out.send(result.unwrap_or_else(Response::Disconnected));
}
