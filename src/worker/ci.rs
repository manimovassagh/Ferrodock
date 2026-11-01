//! Local CI: runs workflows, stops them, and saves their artifacts. A run's
//! events go to the UI as they happen.

use std::sync::Arc;

use super::{Response, Shared, refresh};
use crate::docker::ci::{self, CiEvent, CiOp};
use crate::docker::{DockerClient, Resource, downloads_dir};

pub(super) fn handle(client: &DockerClient, shared: &Arc<Shared>, op: CiOp) {
    let (client, shared) = (client.clone(), shared.clone());
    tokio::spawn(async move {
        match op {
            CiOp::Run(request) => {
                let emit = |event: CiEvent| shared.send(Response::Ci(event));
                ci::run_workflow(&client, *request, &emit).await;
                refresh(&client, &shared, Resource::Containers).await;
            }
            CiOp::Cancel(run) => ci::cancel_run(&client, run).await,
            CiOp::SaveArtifact { run, name } => {
                let saved = client.save_artifact(run, &name, &downloads_dir()).await;
                shared.send(Response::CiSaved(match saved {
                    Ok(file) => Ok(format!("Saved {name} to {}", file.display())),
                    Err(e) => Err(format!("Couldn't save {name}: {e}")),
                }));
            }
            CiOp::Forget(run) => client.remove_artifacts(run).await,
        }
    });
}
