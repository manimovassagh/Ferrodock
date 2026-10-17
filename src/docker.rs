//! Thin wrapper around the Docker Engine API (via `bollard`).
//!
//! On Windows this talks to Docker Desktop / dockerd over the
//! `\\.\pipe\docker_engine` named pipe; elsewhere it uses the unix socket.
//! `DOCKER_HOST` is honoured on every platform.

use std::time::{SystemTime, UNIX_EPOCH};

use bollard::Docker;
use bollard::models::{ContainerSummary, ContainerSummaryStateEnum};
use bollard::query_parameters::{
    ListContainersOptions, RestartContainerOptions, StartContainerOptions, StopContainerOptions,
};

/// UI-friendly snapshot of a container.
#[derive(Debug, Clone, PartialEq)]
pub struct ContainerInfo {
    pub id: String,
    pub name: String,
    pub image: String,
    pub command: String,
    /// Machine-readable state, e.g. `running`, `exited`.
    pub state: String,
    /// Human-readable status, e.g. `Up 5 minutes`.
    pub status: String,
    /// Unix timestamp (seconds).
    pub created: i64,
    pub ports: Vec<String>,
    pub ip_addresses: Vec<String>,
    pub mounts: Vec<String>,
}

impl ContainerInfo {
    pub fn short_id(&self) -> &str {
        &self.id[..self.id.len().min(12)]
    }

    pub fn is_running(&self) -> bool {
        matches!(self.state.as_str(), "running" | "restarting")
    }

    pub fn is_paused(&self) -> bool {
        self.state == "paused"
    }
}

impl From<ContainerSummary> for ContainerInfo {
    fn from(c: ContainerSummary) -> Self {
        let name = c
            .names
            .as_deref()
            .and_then(|n| n.first())
            .map(|n| n.trim_start_matches('/').to_string())
            .unwrap_or_default();

        let mut ports: Vec<String> = c
            .ports
            .unwrap_or_default()
            .iter()
            .map(|p| {
                let proto = p.typ.map(|t| t.to_string()).unwrap_or_else(|| "tcp".into());
                match p.public_port {
                    Some(public) => {
                        let ip = p.ip.as_deref().unwrap_or("0.0.0.0");
                        format!("{ip}:{public} -> {}/{proto}", p.private_port)
                    }
                    None => format!("{}/{proto}", p.private_port),
                }
            })
            .collect();
        ports.sort();
        ports.dedup();

        let ip_addresses = c
            .network_settings
            .and_then(|n| n.networks)
            .map(|nets| {
                let mut ips: Vec<String> = nets
                    .into_iter()
                    .filter_map(|(net, ep)| {
                        ep.ip_address
                            .filter(|ip| !ip.is_empty())
                            .map(|ip| format!("{ip} ({net})"))
                    })
                    .collect();
                ips.sort();
                ips
            })
            .unwrap_or_default();

        let mounts = c
            .mounts
            .unwrap_or_default()
            .into_iter()
            .map(|m| {
                let src = m.name.or(m.source).unwrap_or_default();
                let dst = m.destination.unwrap_or_default();
                format!("{src} -> {dst}")
            })
            .collect();

        Self {
            id: c.id.unwrap_or_default(),
            name,
            image: c.image.unwrap_or_default(),
            command: c.command.unwrap_or_default(),
            state: c
                .state
                .filter(|s| *s != ContainerSummaryStateEnum::EMPTY)
                .map(|s| s.to_string())
                .unwrap_or_else(|| "unknown".into()),
            status: c.status.unwrap_or_default(),
            created: c.created.unwrap_or_default(),
            ports,
            ip_addresses,
            mounts,
        }
    }
}

/// Lifecycle actions the UI can request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Start,
    Stop,
    Restart,
}

impl Action {
    pub fn verb(self) -> &'static str {
        match self {
            Action::Start => "start",
            Action::Stop => "stop",
            Action::Restart => "restart",
        }
    }
}

#[derive(Clone)]
pub struct DockerClient {
    docker: Docker,
}

impl DockerClient {
    /// Creates a client. This does not touch the daemon yet; connection
    /// errors surface on the first request.
    pub fn connect() -> Result<Self, String> {
        Docker::connect_with_defaults()
            .map(|docker| Self { docker })
            .map_err(|e| format!("Could not set up Docker connection: {e}"))
    }

    /// Returns the Docker Engine version, used as a connectivity check.
    pub async fn version(&self) -> Result<String, String> {
        let v = self.docker.version().await.map_err(describe)?;
        Ok(v.version.unwrap_or_else(|| "unknown".into()))
    }

    /// Lists all containers (running and stopped), sorted by name.
    pub async fn list_containers(&self) -> Result<Vec<ContainerInfo>, String> {
        let opts = ListContainersOptions {
            all: true,
            ..Default::default()
        };
        let mut list: Vec<ContainerInfo> = self
            .docker
            .list_containers(Some(opts))
            .await
            .map_err(describe)?
            .into_iter()
            .map(ContainerInfo::from)
            .collect();
        list.sort_by_cached_key(|c| c.name.to_lowercase());
        Ok(list)
    }

    pub async fn perform(&self, action: Action, id: &str) -> Result<(), String> {
        let result = match action {
            Action::Start => {
                self.docker
                    .start_container(id, None::<StartContainerOptions>)
                    .await
            }
            Action::Stop => {
                self.docker
                    .stop_container(id, None::<StopContainerOptions>)
                    .await
            }
            Action::Restart => {
                self.docker
                    .restart_container(id, None::<RestartContainerOptions>)
                    .await
            }
        };
        result.map_err(describe)
    }
}

/// Turns bollard errors into a short message, with a hint when the daemon
/// is simply not reachable (the most common case on a fresh Windows box).
fn describe(err: bollard::errors::Error) -> String {
    use bollard::errors::Error as E;
    match err {
        E::DockerResponseServerError { message, .. } => message,
        E::HyperResponseError { .. } | E::IOError { .. } => {
            format!("Cannot reach the Docker Engine ({err}). Is Docker Desktop running?")
        }
        other => other.to_string(),
    }
}

/// Formats a unix timestamp as a coarse "N units ago" string.
pub fn time_ago(unix_secs: i64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    format_age(now - unix_secs)
}

fn format_age(secs: i64) -> String {
    let (n, unit) = match secs.max(0) {
        s if s < 60 => return "just now".into(),
        s if s < 3_600 => (s / 60, "minute"),
        s if s < 86_400 => (s / 3_600, "hour"),
        s if s < 2_592_000 => (s / 86_400, "day"),
        s if s < 31_536_000 => (s / 2_592_000, "month"),
        s => (s / 31_536_000, "year"),
    };
    let plural = if n == 1 { "" } else { "s" };
    format!("{n} {unit}{plural} ago")
}

#[cfg(test)]
mod tests {
    use super::*;
    use bollard::models::{PortSummary, PortSummaryTypeEnum};

    #[test]
    fn converts_summary() {
        let summary = ContainerSummary {
            id: Some("0123456789abcdef0123".into()),
            names: Some(vec!["/web".into()]),
            image: Some("nginx:latest".into()),
            state: Some(ContainerSummaryStateEnum::RUNNING),
            status: Some("Up 2 minutes".into()),
            ports: Some(vec![
                PortSummary {
                    ip: Some("0.0.0.0".into()),
                    private_port: 80,
                    public_port: Some(8080),
                    typ: Some(PortSummaryTypeEnum::TCP),
                },
                PortSummary {
                    ip: None,
                    private_port: 443,
                    public_port: None,
                    typ: Some(PortSummaryTypeEnum::TCP),
                },
            ]),
            ..Default::default()
        };
        let info = ContainerInfo::from(summary);
        assert_eq!(info.name, "web");
        assert_eq!(info.short_id(), "0123456789ab");
        assert_eq!(info.state, "running");
        assert!(info.is_running());
        assert_eq!(info.ports, vec!["0.0.0.0:8080 -> 80/tcp", "443/tcp"]);
    }

    #[test]
    fn handles_missing_fields() {
        let info = ContainerInfo::from(ContainerSummary::default());
        assert_eq!(info.name, "");
        assert_eq!(info.short_id(), "");
        assert_eq!(info.state, "unknown");
        assert!(!info.is_running());
    }

    #[test]
    fn formats_age() {
        assert_eq!(format_age(5), "just now");
        assert_eq!(format_age(60), "1 minute ago");
        assert_eq!(format_age(7_200), "2 hours ago");
        assert_eq!(format_age(86_400 * 3), "3 days ago");
        assert_eq!(format_age(-10), "just now");
    }

    /// Talks to a real daemon: `cargo test -- --ignored`.
    #[tokio::test]
    #[ignore]
    async fn live_daemon_lists_containers() {
        let client = DockerClient::connect().unwrap();
        client.version().await.unwrap();
        client.list_containers().await.unwrap();
    }
}
