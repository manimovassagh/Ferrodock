//! The cloud emulator: starts, stops, empties and removes it, and asks it
//! what it has. Each ends with an event for the UI.

use std::sync::Arc;

use super::{Response, Shared, refresh};
use crate::docker::cloud::{self, CloudDetail, CloudInfo, CloudOp, CloudRequest};
use crate::docker::{DockerClient, Resource};

/// What the emulator is doing, or said.
#[derive(Debug)]
pub enum CloudEvent {
    /// What starting it is doing, e.g. "Downloading the emulator…".
    Step(String),
    /// What it has, or why it couldn't be asked.
    Info(Result<CloudInfo, String>),
    /// The requests it answered since last asked and the newest ID it
    /// has, or why they couldn't be read.
    Requests(Result<(Vec<CloudRequest>, u64), String>),
    /// A request's headers and bodies, by its ID.
    Detail(u64, Result<CloudDetail, String>),
    Done(CloudOp),
    Failed(CloudOp, String),
}

/// Runs something that waits on the network without holding up the rest.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|e| e.to_string())?
}

pub(super) fn handle(client: &DockerClient, shared: &Arc<Shared>, op: CloudOp) {
    let (client, shared) = (client.clone(), shared.clone());
    tokio::spawn(async move {
        let send = |event: CloudEvent| shared.send(Response::Cloud(event));
        let result = match op {
            CloudOp::Info(port) => {
                send(CloudEvent::Info(blocking(move || cloud::info(port)).await));
                return;
            }
            CloudOp::Requests { port, after } => {
                let read = blocking(move || cloud::requests(port, after)).await;
                send(CloudEvent::Requests(read));
                return;
            }
            CloudOp::Detail { port, id } => {
                let read = blocking(move || cloud::request_detail(port, id)).await;
                send(CloudEvent::Detail(id, read));
                return;
            }
            CloudOp::Start(spec) => {
                client
                    .start_cloud(spec, |step| send(CloudEvent::Step(step.to_owned())))
                    .await
            }
            CloudOp::Stop => client.stop_cloud().await,
            CloudOp::Remove => client.remove_cloud().await,
            CloudOp::Reset(port) => blocking(move || cloud::reset(port)).await,
        };
        send(match result {
            Ok(()) => CloudEvent::Done(op),
            Err(e) => CloudEvent::Failed(op, e),
        });
        refresh(&client, &shared, Resource::Containers).await;
    });
}
