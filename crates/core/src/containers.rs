//! Containers: listing.

use bollard::models::{ContainerSummary, ContainerSummaryStateEnum};
use bollard::query_parameters::ListContainersOptions;

use super::{DockerClient, PortMapping, describe};

/// UI-friendly snapshot of a container.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
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
    pub ports: Vec<PortMapping>,
    pub ip_addresses: Vec<String>,
    /// Names of the networks the container is attached to.
    pub networks: Vec<String>,
    pub mounts: Vec<String>,
    /// Names of the named volumes the container uses.
    pub volumes: Vec<String>,
    /// Set when the container belongs to a Docker Compose project.
    pub compose: Option<ComposeLabels>,
    /// For a Linux machine: its distribution's ID and the folder shared
    /// into it (empty for none).
    pub machine: Option<(String, String)>,
    /// For a node of a Kubernetes cluster: what it records about the
    /// cluster.
    pub kube: Option<KubeLabels>,
}

/// What a node of a Kubernetes cluster records in its labels.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct KubeLabels {
    pub cluster: String,
    pub server: bool,
    pub api_port: u16,
    pub ingress_port: u16,
    /// The kubectl context Ferrodock added; empty for none.
    pub context: String,
}

impl KubeLabels {
    fn from_labels(labels: &std::collections::HashMap<String, String>) -> Option<Self> {
        let get = |key: &str| labels.get(key).cloned().unwrap_or_default();
        Some(Self {
            cluster: labels.get(super::KUBE_LABEL)?.clone(),
            server: get(super::ROLE_LABEL) == "server",
            api_port: get(super::API_LABEL).parse().unwrap_or_default(),
            ingress_port: get(super::INGRESS_LABEL).parse().unwrap_or_default(),
            context: get(super::CONTEXT_LABEL),
        })
    }
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

    /// Running and not paused or restarting, so it can be paused.
    pub fn can_pause(&self) -> bool {
        self.state == "running"
    }

    pub fn is_paused(&self) -> bool {
        self.state == "paused"
    }

    /// Running or paused, i.e. not stopped.
    pub fn is_active(&self) -> bool {
        self.is_running() || self.is_paused()
    }

    /// What the container's healthcheck says, read from Docker's status
    /// ("Up 5 minutes (healthy)"). None without a healthcheck.
    pub fn health(&self) -> Option<HealthStatus> {
        let (_, note) = self.status.rsplit_once('(')?;
        match note.strip_suffix(')')? {
            "healthy" => Some(HealthStatus::Healthy),
            "unhealthy" => Some(HealthStatus::Unhealthy),
            "health: starting" => Some(HealthStatus::Starting),
            _ => None,
        }
    }

    /// Docker's status without the health note, which is shown on its own:
    /// "Up 5 minutes".
    pub fn uptime(&self) -> &str {
        match self.health() {
            Some(_) => self
                .status
                .rsplit_once('(')
                .map_or("", |(up, _)| up.trim_end()),
            None => &self.status,
        }
    }
}

/// A container's healthcheck result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthStatus {
    /// Not checked enough times yet.
    Starting,
    Healthy,
    Unhealthy,
}

impl From<ContainerSummary> for ContainerInfo {
    fn from(c: ContainerSummary) -> Self {
        let name = c
            .names
            .as_deref()
            .and_then(|n| n.first())
            .map(|n| n.trim_start_matches('/').to_string())
            .unwrap_or_default();

        let mut ports: Vec<PortMapping> = c
            .ports
            .unwrap_or_default()
            .iter()
            .map(PortMapping::from)
            .collect();
        // Published ports first. Docker lists a port once for IPv4 and once
        // for IPv6; keep one of them.
        ports.sort_by_key(|p| {
            (
                p.public.is_none(),
                p.public,
                p.private,
                p.proto.clone(),
                p.ip.clone(),
            )
        });
        ports.dedup_by(|a, b| (a.public, a.private, &a.proto) == (b.public, b.private, &b.proto));

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
        let machine = c.labels.as_ref().and_then(|labels| {
            let distro = labels.get(super::MACHINE_LABEL)?.clone();
            let folder = labels.get(super::FOLDER_LABEL).cloned().unwrap_or_default();
            Some((distro, folder))
        });
        let kube = c.labels.as_ref().and_then(KubeLabels::from_labels);

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
            machine,
            kube,
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
            // Helpers for browsing volumes and debug shells are an
            // implementation detail.
            .filter(|c| {
                c.labels.as_ref().is_none_or(|l| {
                    !l.contains_key(super::browse::HELPER_LABEL)
                        && !l.contains_key(super::debug::DEBUG_LABEL)
                })
            })
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
    fn reads_health_from_the_status() {
        let with = |status: &str| ContainerInfo {
            status: status.into(),
            ..Default::default()
        };
        let c = with("Up 5 minutes (healthy)");
        assert_eq!(c.health(), Some(HealthStatus::Healthy));
        assert_eq!(c.uptime(), "Up 5 minutes");
        let c = with("Up 3 seconds (health: starting)");
        assert_eq!(c.health(), Some(HealthStatus::Starting));
        assert_eq!(c.uptime(), "Up 3 seconds");
        assert_eq!(
            with("Up 2 hours (unhealthy)").health(),
            Some(HealthStatus::Unhealthy)
        );
        // No healthcheck, or a note that isn't one.
        for status in [
            "Up 5 minutes",
            "Up 1 second (Paused)",
            "Exited (0) 2 hours ago",
        ] {
            assert_eq!(with(status).health(), None, "{status}");
            assert_eq!(with(status).uptime(), status);
        }
    }

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
        let labels: Vec<String> = info.ports.iter().map(PortMapping::label).collect();
        assert_eq!(labels, vec!["0.0.0.0:8080 -> 80/tcp", "443/tcp"]);
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
