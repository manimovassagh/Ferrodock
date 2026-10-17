//! Running [`FileOp`]s: listing, previewing and copying files in and out.

use super::{Response, Shared};
use crate::docker::{DockerClient, FileOp, downloads_dir, plural};

pub(super) async fn handle(client: &DockerClient, shared: &Shared, op: FileOp) {
    let response = match op {
        FileOp::List { container, path } => Response::Files(
            client
                .list_files(&container, &path)
                .await
                .map_err(|e| (container, path, e)),
        ),
        FileOp::Preview { container, path } => {
            let preview = client.preview_file(&container, &path).await;
            Response::FilePreview {
                container,
                path,
                preview,
            }
        }
        FileOp::Download { container, path } => {
            let name = path.rsplit('/').find(|s| !s.is_empty()).unwrap_or("/");
            match client
                .download_file(&container, &path, &downloads_dir())
                .await
            {
                Ok(saved) => Response::Transferred {
                    container,
                    changed_dir: None,
                    text: format!("Saved {name} to {}", saved.display()),
                    is_error: false,
                },
                Err(e) => Response::Transferred {
                    container,
                    changed_dir: None,
                    text: format!("Couldn't download {name}: {e}"),
                    is_error: true,
                },
            }
        }
        FileOp::BrowseVolume { volume } => {
            let result = client.volume_helper(&volume).await;
            Response::VolumeHelper { volume, result }
        }
        FileOp::EndBrowse { volume } => {
            // Nothing to tell: a helper that's left is removed next time.
            let _ = client.remove_helpers(Some(&volume)).await;
            return;
        }
        FileOp::Upload {
            container,
            dir,
            files,
        } => {
            let what = match files.as_slice() {
                [one] => one
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                many => plural(many.len(), "item"),
            };
            let (text, is_error) = match client.upload_files(&container, &dir, &files).await {
                Ok(()) => (format!("Copied {what} to {dir}"), false),
                Err(e) => (format!("Couldn't copy {what} to {dir}: {e}"), true),
            };
            Response::Transferred {
                container,
                changed_dir: (!is_error).then_some(dir),
                text,
                is_error,
            }
        }
    };
    shared.send(response);
}
