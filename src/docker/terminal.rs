//! Interactive shells inside containers (`docker exec -it`).

use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};

use bollard::container::LogOutput;
use bollard::exec::{CreateExecOptions, ResizeExecOptions, StartExecOptions, StartExecResults};
use futures_util::Stream;
use tokio::io::AsyncWrite;

use super::{DockerClient, describe};

/// Picks bash when the container has it, sh otherwise.
const AUTO_SHELL: &str = "command -v bash >/dev/null 2>&1 && exec bash || exec sh";

/// Something to do with a terminal session. Sessions are numbered by
/// [`next_session`] so answers can be matched to the session that asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TermOp {
    /// Start a shell. An empty `shell` picks bash, then sh.
    Open {
        session: u64,
        container: String,
        shell: String,
        cols: u16,
        rows: u16,
    },
    /// Keys the user typed, already encoded for a terminal.
    Input {
        session: u64,
        bytes: Vec<u8>,
    },
    Resize {
        session: u64,
        cols: u16,
        rows: u16,
    },
    Close {
        session: u64,
    },
}

/// A fresh session number.
pub fn next_session() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

pub type ShellOutput =
    Pin<Box<dyn Stream<Item = Result<LogOutput, bollard::errors::Error>> + Send>>;

/// A running shell: what it prints, and where to send keys.
pub struct Shell {
    pub exec_id: String,
    pub output: ShellOutput,
    pub input: Pin<Box<dyn AsyncWrite + Send>>,
}

/// What to tell the user when a shell exits with `code`. `None` for a
/// normal exit.
pub fn exit_message(code: Option<i64>) -> Option<String> {
    match code {
        Some(0) => None,
        Some(126 | 127) => Some(
            "This container has no shell, or not the one asked for. Images built \
             \"distroless\" or FROM scratch can't be opened this way."
                .into(),
        ),
        Some(code) => Some(format!("The shell exited with code {code}.")),
        None => Some("The connection to the shell was lost.".into()),
    }
}

impl DockerClient {
    pub async fn open_shell(
        &self,
        container: &str,
        shell: &str,
        cols: u16,
        rows: u16,
    ) -> Result<Shell, String> {
        let cmd = if shell.trim().is_empty() {
            vec!["/bin/sh".into(), "-c".into(), AUTO_SHELL.into()]
        } else {
            shell.split_whitespace().map(str::to_owned).collect()
        };
        let exec = self
            .docker
            .create_exec(
                container,
                CreateExecOptions {
                    cmd: Some(cmd),
                    env: Some(vec!["TERM=xterm-256color".into()]),
                    tty: Some(true),
                    attach_stdin: Some(true),
                    attach_stdout: Some(true),
                    attach_stderr: Some(true),
                    ..Default::default()
                },
            )
            .await
            .map_err(describe)?;
        let started = self
            .docker
            .start_exec(
                &exec.id,
                Some(StartExecOptions {
                    tty: true,
                    ..Default::default()
                }),
            )
            .await
            .map_err(describe)?;
        let StartExecResults::Attached { output, input } = started else {
            return Err("The shell didn't attach".into());
        };
        // Best effort: a shell that already exited can't be resized.
        let _ = self.resize_shell(&exec.id, cols, rows).await;
        Ok(Shell {
            exec_id: exec.id,
            output,
            input,
        })
    }

    pub async fn resize_shell(&self, exec_id: &str, cols: u16, rows: u16) -> Result<(), String> {
        self.docker
            .resize_exec(
                exec_id,
                ResizeExecOptions {
                    width: cols,
                    height: rows,
                },
            )
            .await
            .map_err(describe)
    }

    pub async fn shell_exit_code(&self, exec_id: &str) -> Option<i64> {
        self.docker.inspect_exec(exec_id).await.ok()?.exit_code
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explains_exits() {
        assert_eq!(exit_message(Some(0)), None);
        assert!(exit_message(Some(127)).unwrap().contains("no shell"));
        assert!(exit_message(Some(3)).unwrap().contains("code 3"));
    }

    #[test]
    fn sessions_are_unique() {
        assert_ne!(next_session(), next_session());
    }

    /// Talks to a real daemon: `cargo test -- --ignored`. Opens a shell in
    /// a fresh container and drives it like the Terminal tab does: a
    /// command, the size, `top` and `vi` (full-screen), then exit. A
    /// missing shell gets the "no shell" message.
    #[tokio::test]
    #[ignore]
    async fn live_daemon_runs_an_interactive_shell() {
        use futures_util::StreamExt;
        use tokio::io::AsyncWriteExt;

        let client = DockerClient::connect().unwrap();
        let Some(image) = client.list_images().await.unwrap().into_iter().next() else {
            return;
        };
        let spec = crate::docker::RunSpec {
            image: image.id,
            name: "ferrodock-terminal".into(),
            command: "sleep 300".into(),
            ..Default::default()
        };
        let opts = || bollard::query_parameters::RemoveContainerOptions {
            force: true,
            ..Default::default()
        };
        let _ = client
            .docker
            .remove_container(&spec.name, Some(opts()))
            .await;
        let id = client.run(&spec).await.unwrap();

        let mut sh = client.open_shell(&id, "", 100, 30).await.unwrap();
        let mut screen = vt100::Parser::new(30, 100, 0);
        // Types `keys`, then reads until the screen shows `want`.
        async fn step(sh: &mut Shell, screen: &mut vt100::Parser, keys: &[u8], want: &str) -> bool {
            sh.input.write_all(keys).await.unwrap();
            sh.input.flush().await.unwrap();
            let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
            while !screen.screen().contents().contains(want) {
                match tokio::time::timeout_at(deadline, sh.output.next()).await {
                    Ok(Some(Ok(out))) => screen.process(&out.into_bytes()),
                    _ => return false,
                }
            }
            true
        }
        let math = step(&mut sh, &mut screen, b"echo $((6*7))\r", "42").await;
        let size = step(&mut sh, &mut screen, b"stty size\r", "30 100").await;
        let top = step(&mut sh, &mut screen, b"top\r", "PID").await;
        let quit_top = step(&mut sh, &mut screen, b"q", "PID").await
            && step(&mut sh, &mut screen, b"echo after-$((1+1))\r", "after-2").await;
        let vi = step(&mut sh, &mut screen, b"vi /etc/ferrodock\r", "~").await;
        let typed = step(&mut sh, &mut screen, b"ihello vi\x1b", "hello vi").await;
        let quit_vi = step(&mut sh, &mut screen, b":q!\r", "~").await
            && step(&mut sh, &mut screen, b"echo back-$((2+2))\r", "back-4").await;
        sh.input.write_all(b"exit\r").await.unwrap();
        while sh.output.next().await.is_some() {}
        let code = client.shell_exit_code(&sh.exec_id).await;

        let mut missing = client
            .open_shell(&id, "/no/such/shell", 80, 24)
            .await
            .unwrap();
        while missing.output.next().await.is_some() {}
        let missing_code = client.shell_exit_code(&missing.exec_id).await;

        client
            .docker
            .remove_container(&id, Some(opts()))
            .await
            .unwrap();
        assert!(math, "echo works");
        assert!(size, "the shell sees the size we set");
        assert!(top && quit_top, "top runs and quits");
        assert!(vi && typed && quit_vi, "vi opens, takes text and quits");
        assert_eq!(exit_message(code), None);
        assert!(exit_message(missing_code).unwrap().contains("no shell"));
    }
}
