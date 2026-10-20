//! Driving the `docker compose` command: bringing projects up from their
//! files, rebuilding services, and following merged logs. The Engine API
//! has no Compose support, so this is the one place Ferrodock runs a
//! program, pointed at the engine it's using: Ferrodock's own copy of
//! Compose when it has one, otherwise the Docker CLI's `docker compose`.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

use super::Engine;

/// A fresh job number, to match output to the job that printed it.
pub fn next_job() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// A Compose project: its files, folder and (optionally) name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposeTarget {
    /// `-p`; None lets Compose name it after the folder.
    pub name: Option<String>,
    pub files: Vec<PathBuf>,
    /// Where relative paths in the files resolve.
    pub dir: PathBuf,
}

impl ComposeTarget {
    /// A project from one file, named after its folder like Compose does.
    pub fn from_file(file: &Path) -> Self {
        Self {
            name: None,
            files: vec![file.to_owned()],
            dir: file.parent().map(Path::to_owned).unwrap_or_default(),
        }
    }

    /// What to call it in messages.
    pub fn label(&self) -> String {
        match &self.name {
            Some(name) => name.clone(),
            None => self
                .dir
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "project".into()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComposeAction {
    /// `up -d`: create and start everything.
    Up,
    /// Rebuild and recreate one service (or all of them).
    Rebuild(Option<String>),
    /// Pull newer images and recreate the containers.
    PullRecreate,
}

/// Something to run with `docker compose`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComposeOp {
    Run {
        job: u64,
        target: ComposeTarget,
        action: ComposeAction,
    },
    /// Follow the merged logs of a project, by name.
    Logs { job: u64, project: String },
    /// Stop a job (e.g. logs nobody is looking at any more).
    Cancel { job: u64 },
}

/// The arguments after `docker compose` for one step of `action`. Pulling
/// and recreating takes two steps.
pub fn steps(target: &ComposeTarget, action: &ComposeAction) -> Vec<Vec<String>> {
    let mut base = Vec::new();
    if let Some(name) = &target.name {
        base.extend(["-p".into(), name.clone()]);
    }
    for file in &target.files {
        base.extend(["-f".into(), file.display().to_string()]);
    }
    let with = |rest: &[&str]| {
        let mut args = base.clone();
        args.extend(rest.iter().map(|s| (*s).to_owned()));
        args
    };
    match action {
        ComposeAction::Up => vec![with(&["up", "-d", "--remove-orphans"])],
        ComposeAction::Rebuild(Some(service)) => {
            vec![with(&["up", "-d", "--build", "--no-deps", service])]
        }
        ComposeAction::Rebuild(None) => vec![with(&["up", "-d", "--build"])],
        ComposeAction::PullRecreate => vec![
            with(&["pull"]),
            with(&["up", "-d", "--force-recreate", "--remove-orphans"]),
        ],
    }
}

/// `docker compose -p <project> logs -f`: the last lines of every service,
/// then everything new.
pub fn logs_args(project: &str) -> Vec<String> {
    ["-p", project, "logs", "-f", "--tail", "200", "--no-color"]
        .map(str::to_owned)
        .to_vec()
}

/// What runs Compose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Program {
    /// The Docker CLI: `docker compose …`.
    Docker,
    /// Compose on its own, `docker-compose …`: Ferrodock's copy.
    Standalone(PathBuf),
}

/// Ferrodock's copy of Compose when it has one, else the `docker`
/// command when there is one. None: neither, so Compose has to be
/// downloaded first.
pub fn program() -> Option<Program> {
    if let Some(own) = super::builtin::Paths::locate().map(|p| p.compose())
        && own.is_file()
    {
        return Some(Program::Standalone(own));
    }
    on_path("docker").then_some(Program::Docker)
}

/// Whether `name` is a program in PATH.
fn on_path(name: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    let names: Vec<String> = if cfg!(windows) {
        vec![format!("{name}.exe"), format!("{name}.cmd")]
    } else {
        vec![name.to_owned()]
    };
    std::env::split_paths(&path).any(|dir| names.iter().any(|n| dir.join(n).is_file()))
}

/// A `docker compose` command aimed at `engine` (or the CLI's own default).
pub fn command(engine: Option<&Engine>, dir: Option<&Path>, args: &[String]) -> Command {
    let mut cmd = match program() {
        Some(Program::Standalone(own)) => Command::new(own),
        _ => {
            let mut docker = Command::new("docker");
            docker.arg("compose");
            docker
        }
    };
    cmd.args(args);
    if let Some(dir) = dir.filter(|d| d.is_dir()) {
        cmd.current_dir(dir);
    }
    if let Some(engine) = engine {
        cmd.env_remove("DOCKER_CONTEXT")
            .env("DOCKER_HOST", &engine.host);
        if let Some((ca, _, _)) = &engine.tls {
            cmd.env("DOCKER_TLS_VERIFY", "1");
            if let Some(certs) = ca.parent() {
                cmd.env("DOCKER_CERT_PATH", certs);
            }
        }
    }
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    cmd
}

/// Runs `cmd`, calling `line` for each line it prints (stdout and stderr
/// merged). Returns whether it succeeded.
pub async fn stream(mut cmd: Command, mut line: impl FnMut(String)) -> Result<bool, String> {
    let mut child: Child = cmd.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            "Docker Compose isn't installed".to_owned()
        } else {
            e.to_string()
        }
    })?;
    let mut out = BufReader::new(child.stdout.take().ok_or("no output")?).lines();
    let mut err = BufReader::new(child.stderr.take().ok_or("no output")?).lines();
    let (mut out_open, mut err_open) = (true, true);
    while out_open || err_open {
        tokio::select! {
            next = out.next_line(), if out_open => match next {
                Ok(Some(text)) => line(text),
                _ => out_open = false,
            },
            next = err.next_line(), if err_open => match next {
                Ok(Some(text)) => line(text),
                _ => err_open = false,
            },
        }
    }
    let status = child.wait().await.map_err(|e| e.to_string())?;
    Ok(status.success())
}

/// `web-1  | listening on :80` → ("web", "listening on :80"). Lines that
/// aren't from a service give an empty service.
pub fn split_log_line(line: &str) -> (&str, &str) {
    match line.split_once(" | ") {
        Some((prefix, text)) => {
            let container = prefix.trim();
            // Compose names containers <service>-<n>.
            let service = match container.rsplit_once('-') {
                Some((service, n)) if n.chars().all(|c| c.is_ascii_digit()) => service,
                _ => container,
            };
            (service, text)
        }
        None => ("", line),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target() -> ComposeTarget {
        ComposeTarget {
            name: Some("shop".into()),
            files: vec![PathBuf::from("/src/shop/compose.yaml")],
            dir: PathBuf::from("/src/shop"),
        }
    }

    #[test]
    fn builds_the_commands() {
        let up = steps(&target(), &ComposeAction::Up);
        assert_eq!(
            up[0].join(" "),
            "-p shop -f /src/shop/compose.yaml up -d --remove-orphans"
        );
        let rebuild = steps(&target(), &ComposeAction::Rebuild(Some("web".into())));
        assert!(
            rebuild[0]
                .join(" ")
                .ends_with("up -d --build --no-deps web")
        );
        let pull = steps(&target(), &ComposeAction::PullRecreate);
        assert_eq!(pull.len(), 2);
        assert!(pull[0].ends_with(&["pull".to_owned()]));
        assert_eq!(logs_args("shop")[..4], ["-p", "shop", "logs", "-f"]);
    }

    #[test]
    fn names_projects_after_their_folder() {
        let t = ComposeTarget::from_file(Path::new("/src/blog/compose.yaml"));
        assert_eq!(t.label(), "blog");
        assert!(steps(&t, &ComposeAction::Up)[0][0] == "-f");
    }

    #[test]
    fn splits_log_lines() {
        assert_eq!(split_log_line("web-1  | listening"), ("web", "listening"));
        assert_eq!(split_log_line("db-worker-12 | ok"), ("db-worker", "ok"));
        assert_eq!(split_log_line("plain"), ("", "plain"));
    }

    /// Runs the real `docker compose` against the default engine: brings a
    /// one-service project up from a file, rebuilds it, follows its logs,
    /// then takes it down.
    #[tokio::test]
    #[ignore]
    async fn live_compose_up_and_logs() {
        let client = super::super::DockerClient::connect().unwrap();
        let Some(image) = client.list_images().await.unwrap().into_iter().next() else {
            return;
        };
        let tag = image.tags.first().cloned().unwrap_or(image.id);
        let dir = std::env::temp_dir().join(format!("ferrodock-compose-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("compose.yaml");
        std::fs::write(
            &file,
            format!(
                "services:\n  hello:\n    image: {tag}\n    command: sh -c 'echo hello from compose; sleep 300'\n"
            ),
        )
        .unwrap();
        let target = ComposeTarget {
            name: Some("ferrodock-compose-test".into()),
            ..ComposeTarget::from_file(&file)
        };
        let mut output = Vec::new();
        let mut ok = true;
        for args in steps(&target, &ComposeAction::Up) {
            ok &= stream(command(None, Some(&dir), &args), |l| output.push(l))
                .await
                .unwrap();
        }
        // Follow the logs until the service's line shows up.
        let mut logs = Vec::new();
        let mut follow = command(None, None, &logs_args("ferrodock-compose-test"));
        follow.kill_on_drop(true);
        let _ = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            stream(follow, |l| logs.push(l)),
        )
        .await;
        let down = ["-p", "ferrodock-compose-test", "down", "-t", "1"].map(str::to_owned);
        stream(command(None, None, &down), |_| {}).await.unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(ok, "up failed: {output:?}");
        assert!(
            logs.iter()
                .any(|l| split_log_line(l) == ("hello", "hello from compose")),
            "{logs:?}"
        );
    }
}
