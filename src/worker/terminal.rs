//! Terminal sessions: one task per open shell, fed keys and size changes
//! through a channel and sending back what the shell prints.

use std::collections::HashMap;
use std::sync::Arc;

use futures_util::StreamExt;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;

use super::{Response, Shared};
use crate::docker::{DockerClient, TermOp, exit_message};

enum Msg {
    Input(Vec<u8>),
    Resize(u16, u16),
}

#[derive(Default)]
pub(super) struct Sessions {
    open: HashMap<u64, (mpsc::UnboundedSender<Msg>, tokio::task::JoinHandle<()>)>,
}

impl Sessions {
    pub(super) fn handle(&mut self, client: &DockerClient, shared: &Arc<Shared>, op: TermOp) {
        self.open.retain(|_, (_, task)| !task.is_finished());
        match op {
            TermOp::Open {
                session,
                container,
                shell,
                cols,
                rows,
            } => {
                let (tx, rx) = mpsc::unbounded_channel();
                let (client, shared) = (client.clone(), shared.clone());
                let task = tokio::spawn(async move {
                    run(
                        &client,
                        &shared,
                        session,
                        &container,
                        &shell,
                        (cols, rows),
                        rx,
                    )
                    .await;
                });
                self.open.insert(session, (tx, task));
            }
            TermOp::Input { session, bytes } => self.send(session, Msg::Input(bytes)),
            TermOp::Resize {
                session,
                cols,
                rows,
            } => self.send(session, Msg::Resize(cols, rows)),
            TermOp::Close { session } => {
                if let Some((_, task)) = self.open.remove(&session) {
                    // Dropping the connection hangs up the shell.
                    task.abort();
                }
            }
        }
    }

    fn send(&self, session: u64, msg: Msg) {
        if let Some((tx, _)) = self.open.get(&session) {
            let _ = tx.send(msg);
        }
    }
}

async fn run(
    client: &DockerClient,
    shared: &Shared,
    session: u64,
    container: &str,
    shell: &str,
    (cols, rows): (u16, u16),
    mut keys: mpsc::UnboundedReceiver<Msg>,
) {
    let mut sh = match client.open_shell(container, shell, cols, rows).await {
        Ok(sh) => sh,
        Err(e) => {
            shared.send(Response::TermExit {
                session,
                message: Some(format!("Couldn't open a shell: {e}")),
            });
            return;
        }
    };
    loop {
        tokio::select! {
            chunk = sh.output.next() => match chunk {
                Some(Ok(output)) => shared.send(Response::TermOutput {
                    session,
                    bytes: output.into_bytes().to_vec(),
                }),
                Some(Err(_)) | None => break,
            },
            msg = keys.recv() => match msg {
                Some(Msg::Input(bytes)) => {
                    if sh.input.write_all(&bytes).await.is_err() {
                        break;
                    }
                    let _ = sh.input.flush().await;
                }
                Some(Msg::Resize(cols, rows)) => {
                    let _ = client.resize_shell(&sh.exec_id, cols, rows).await;
                }
                None => break,
            },
        }
    }
    let code = client.shell_exit_code(&sh.exec_id).await;
    shared.send(Response::TermExit {
        session,
        message: exit_message(code),
    });
}
