//! Streams logs and stats for the one container being watched.

use std::sync::Arc;

use futures_util::StreamExt;
use tokio::task::AbortHandle;

use super::{Response, Sender};
use crate::docker::DockerClient;

/// Log lines loaded when watching starts.
const LOG_TAIL: usize = 500;
/// Log chunks already waiting are forwarded together, so a chatty
/// container causes one repaint per batch instead of one per line.
const LOG_BATCH: usize = 256;

/// The streams of the currently watched container.
#[derive(Default)]
pub struct Watch {
    tasks: Vec<AbortHandle>,
}

impl Watch {
    /// Starts streaming `container_id`, replacing any previous watch.
    pub fn start(&mut self, client: &DockerClient, out: &Arc<Sender>, container_id: String) {
        self.stop();
        let logs = tokio::spawn(stream_logs(
            client.clone(),
            out.clone(),
            container_id.clone(),
        ));
        let stats = tokio::spawn(stream_stats(client.clone(), out.clone(), container_id));
        self.tasks = vec![logs.abort_handle(), stats.abort_handle()];
    }

    pub fn stop(&mut self) {
        for task in self.tasks.drain(..) {
            task.abort();
        }
    }
}

async fn stream_logs(client: DockerClient, out: Arc<Sender>, container_id: String) {
    let mut batches = std::pin::pin!(client.logs(&container_id, LOG_TAIL).ready_chunks(LOG_BATCH));
    while let Some(batch) = batches.next().await {
        let mut text = String::new();
        let mut ended = false;
        for chunk in batch {
            match chunk {
                Ok(chunk) => text.push_str(&chunk),
                Err(_) => ended = true,
            }
        }
        if !text.is_empty() {
            out.send(Response::Logs {
                container_id: container_id.clone(),
                text,
            });
        }
        if ended {
            break;
        }
    }
}

async fn stream_stats(client: DockerClient, out: Arc<Sender>, container_id: String) {
    let mut readings = std::pin::pin!(client.stats(&container_id));
    while let Some(Ok(sample)) = readings.next().await {
        out.send(Response::Stats {
            container_id: container_id.clone(),
            sample,
        });
    }
}
