//! Running `docker compose` jobs: up, rebuild, pull & recreate, and
//! following logs. Output goes back in batches, at most ten a second.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::{Response, Shared, refresh};
use crate::docker::compose_cli::{command, logs_args, steps, stream};
use crate::docker::compose_install;
use crate::docker::{ComposeOp, DockerClient, Engine, Resource};

const FLUSH_EVERY: Duration = Duration::from_millis(100);

#[derive(Default)]
pub(super) struct Jobs {
    running: HashMap<u64, tokio::task::JoinHandle<()>>,
}

impl Jobs {
    pub(super) fn handle(
        &mut self,
        client: &DockerClient,
        shared: &Arc<Shared>,
        engine: Option<Engine>,
        op: ComposeOp,
    ) {
        self.running.retain(|_, task| !task.is_finished());
        let (client, shared) = (client.clone(), shared.clone());
        match op {
            ComposeOp::Run {
                job,
                target,
                action,
            } => {
                let task = tokio::spawn(async move {
                    let mut out = Batch::new(&shared, job);
                    let mut result = ensure_compose(&mut out).await.map(|()| true);
                    for args in steps(&target, &action) {
                        if !matches!(result, Ok(true)) {
                            break;
                        }
                        out.push(format!("$ docker compose {}", args.join(" ")));
                        let cmd = command(engine.as_ref(), Some(&target.dir), &args);
                        result = stream(cmd, |line| out.push(line)).await;
                    }
                    out.flush();
                    let error = match result {
                        Ok(true) => None,
                        Ok(false) => Some("docker compose failed; see its output".into()),
                        Err(e) => Some(e),
                    };
                    shared.send(Response::ComposeDone { job, error });
                    refresh(&client, &shared, Resource::Containers).await;
                });
                self.running.insert(job, task);
            }
            ComposeOp::Logs { job, project } => {
                let task = tokio::spawn(async move {
                    let mut out = Batch::new(&shared, job);
                    let result = match ensure_compose(&mut out).await {
                        Ok(()) => {
                            let cmd = command(engine.as_ref(), None, &logs_args(&project));
                            stream(cmd, |line| out.push(line)).await
                        }
                        Err(e) => Err(e),
                    };
                    out.flush();
                    shared.send(Response::ComposeDone {
                        job,
                        error: result.err(),
                    });
                });
                self.running.insert(job, task);
            }
            ComposeOp::Cancel { job } => {
                // Dropping the child kills `docker compose`.
                if let Some(task) = self.running.remove(&job) {
                    task.abort();
                }
            }
        }
    }

    /// Stops every job, e.g. when switching engines.
    pub(super) fn cancel_all(&mut self) {
        for (_, task) in self.running.drain() {
            task.abort();
        }
    }
}

/// Downloads Ferrodock's copy of Compose first when there's neither one
/// nor a `docker` command.
async fn ensure_compose(out: &mut Batch<'_>) -> Result<(), String> {
    compose_install::ensure(|| {
        out.push(format!(
            "Downloading Docker Compose {} (once)…",
            compose_install::VERSION
        ));
        out.flush();
    })
    .await
}

/// Output lines, sent together.
pub(super) struct Batch<'a> {
    shared: &'a Shared,
    job: u64,
    lines: Vec<String>,
    sent: Instant,
}

impl<'a> Batch<'a> {
    pub(super) fn new(shared: &'a Shared, job: u64) -> Self {
        Self {
            shared,
            job,
            lines: Vec::new(),
            sent: Instant::now(),
        }
    }

    pub(super) fn push(&mut self, line: String) {
        self.lines.push(line);
        if self.sent.elapsed() >= FLUSH_EVERY {
            self.flush();
        }
    }

    pub(super) fn flush(&mut self) {
        if !self.lines.is_empty() {
            self.shared.send(Response::ComposeOutput {
                job: self.job,
                lines: std::mem::take(&mut self.lines),
            });
        }
        self.sent = Instant::now();
    }
}
