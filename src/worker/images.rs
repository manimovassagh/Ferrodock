//! Running [`ImageOp`]s: search, tag, push, save, load and build.

use std::time::{Duration, Instant};

use super::{Response, Shared, refresh};
use crate::docker::{Action, DockerClient, ImageOp, Resource, Task, downloads_dir, plural};

pub(super) async fn handle(client: &DockerClient, shared: &Shared, op: ImageOp) {
    let (task, result) = match op {
        ImageOp::Build(spec) => {
            let mut out = super::compose::Batch::new(shared, spec.job);
            let result = client.build_image(&spec, |line| out.push(line)).await;
            out.flush();
            shared.send(Response::ComposeDone {
                job: spec.job,
                error: result.err(),
            });
            refresh(client, shared, Resource::Images).await;
            return;
        }
        ImageOp::Search(term) => {
            let results = client.search_images(&term).await;
            shared.send(Response::SearchResults { term, results });
            return;
        }
        ImageOp::Tag { image, target } => {
            let task = task(Action::Tag, &image, &format!("as {}", target.trim()));
            (task, client.tag_image(&image, &target).await)
        }
        ImageOp::Push(reference) => {
            let task = task(Action::Push, &reference, &reference);
            let mut last = Instant::now() - Duration::from_secs(1);
            let result = client
                .push_image(&reference, |progress| {
                    if last.elapsed() >= Duration::from_millis(100) {
                        last = Instant::now();
                        shared.send(Response::Pulling {
                            image: reference.clone(),
                            progress,
                        });
                    }
                })
                .await;
            (task, result)
        }
        ImageOp::Save(image) => match client.save_image(&image, &downloads_dir()).await {
            Ok(path) => {
                let name = format!("{image} to {}", path.display());
                (task(Action::Save, &image, &name), Ok(()))
            }
            Err(e) => (task(Action::Save, &image, &image), Err(e)),
        },
        ImageOp::Load(path) => {
            let file = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let id = path.display().to_string();
            match client.load_image(&path).await {
                Ok(loaded) => {
                    let what = match loaded.as_slice() {
                        [one] => one.trim_start_matches("Loaded image: ").to_owned(),
                        many => format!("{} from {file}", plural(many.len(), "image")),
                    };
                    (task(Action::Load, &id, &what), Ok(()))
                }
                Err(e) => (task(Action::Load, &id, &file), Err(e)),
            }
        }
    };
    match result {
        Ok(()) => shared.send(Response::Done { task, freed: None }),
        Err(e) => {
            let message = format!("Couldn't {} {}: {e}", task.action.verb(), task.name);
            shared.send(Response::Failed { task, message });
        }
    }
    refresh(client, shared, Resource::Images).await;
}

fn task(action: Action, id: &str, name: &str) -> Task {
    Task {
        action,
        id: id.to_owned(),
        name: name.to_owned(),
    }
}
