//! Containers: listing.

use bollard::models::{ContainerSummary, ContainerSummaryStateEnum};
use bollard::query_parameters::ListContainersOptions;

use super::{DockerClient, describe};

/// UI-friendly snapshot of a container.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ContainerInfo {
    pub id: String,
    pub name: String,
    pub image: String,
    /// Full ID of the image the container was created from.
    pub image_id: String,
    pub command: String,
    /// Machine-readable state, e.g. `running`, `exited`.
    pub state: String,
    /// Human-readable status, e.g. `Up 5 minutes`.
    pub status: String,
    /// Unix timestamp (seconds).
    pub created: i64,
    pub ports: Vec<String>,
    pub ip_addresses: Vec<String>,
    /// Names of the networks the container is attached to.
    pub networks: Vec<String>,
    pub mounts: Vec<String>,
    /// Names of the named volumes the container uses.
    pub volumes: Vec<String>,
    /// Set when the container belongs to a Docker Compose project.
    pub compose: Option<ComposeLabels>,
}

/// What Docker Compose records about a container in its labels.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct ComposeLabels {
    pub project: String,
    pub service: String,
    pub working_dir: String,
    pub config_files: String,
}

impl ComposeLabels {
    fn from_labels(labels: &std::collections::HashMap<String, String>) -> Option<Self> {
        let get = |key: &str| labels.get(key).cloned().unwrap_or_default();
        let project = labels.get("com.docker.compose.project")?.clone();
        Some(Self {
            project,
            service: get("com.docker.compose.service"),
            working_dir: get("com.docker.compose.project.working_dir"),
            config_files: get("com.docker.compose.project.config_files"),
        })
    }
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

    /// Running or paused, i.e. not stopped.
    pub fn is_active(&self) -> bool {
        self.is_running() || self.is_paused()
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

        let networks: Vec<(String, String)> = c
            .network_settings
            .and_then(|n| n.networks)
            .map(|nets| {
                let mut nets: Vec<(String, String)> = nets
                    .into_iter()
                    .map(|(net, ep)| (net, ep.ip_address.unwrap_or_default()))
                    .collect();
                nets.sort();
                nets
            })
            .unwrap_or_default();
        let ip_addresses = networks
            .iter()
            .filter(|(_, ip)| !ip.is_empty())
            .map(|(net, ip)| format!("{ip} ({net})"))
            .collect();

        let mut volumes = Vec::new();
        let mounts = c
            .mounts
            .unwrap_or_default()
            .into_iter()
            .map(|m| {
                if let Some(name) = &m.name {
                    volumes.push(name.clone());
                }
                let src = m.name.or(m.source).unwrap_or_default();
                let dst = m.destination.unwrap_or_default();
                format!("{src} -> {dst}")
            })
            .collect();

        let compose = c.labels.as_ref().and_then(ComposeLabels::from_labels);

        Self {
            id: c.id.unwrap_or_default(),
            name,
            image: c.image.unwrap_or_default(),
            image_id: c.image_id.unwrap_or_default(),
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
            networks: networks.into_iter().map(|(net, _)| net).collect(),
            mounts,
            volumes,
            compose,
        }
    }
}

impl DockerClient {
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

    /// Talks to a real daemon: `cargo test -- --ignored`.
    #[tokio::test]
    #[ignore]
    async fn live_daemon_lists_containers() {
        let client = DockerClient::connect().unwrap();
        client.version().await.unwrap();
        client.list_containers().await.unwrap();
    }
}
