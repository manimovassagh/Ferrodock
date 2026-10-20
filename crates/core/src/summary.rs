//! Compact views of what's on an engine, for the `fdock` CLI's `--json`
//! and the MCP server: one schema for both, short enough to be cheap for
//! AI tools to read. Sizes and times are human-readable; empty fields are
//! left out.

use serde::Serialize;

use super::{
    ComposeProject, ContainerInfo, DiskUsage, ImageInfo, Inspection, NetworkInfo, PortMapping,
    Usage, VolumeInfo, format_size, looks_secret, time_ago,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Container {
    pub name: String,
    pub id: String,
    pub image: String,
    /// `running`, `exited`, `paused`…
    pub state: String,
    /// e.g. `Up 5 minutes`.
    pub status: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ports: Vec<String>,
    /// Its Docker Compose project.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
}

/// `8080->80/tcp`, with the address when it isn't all of them; `80/tcp`
/// when not published.
pub fn port(p: &PortMapping) -> String {
    match p.public {
        Some(public) if p.ip.is_empty() || p.ip == "0.0.0.0" || p.ip == "::" => {
            format!("{public}->{}/{}", p.private, p.proto)
        }
        Some(public) => format!("{}:{public}->{}/{}", p.ip, p.private, p.proto),
        None => format!("{}/{}", p.private, p.proto),
    }
}

impl From<&ContainerInfo> for Container {
    fn from(c: &ContainerInfo) -> Self {
        let mut ports: Vec<String> = c.ports.iter().map(port).collect();
        // IPv4 and IPv6 bindings of the same port read the same.
        ports.dedup();
        Self {
            name: c.name.clone(),
            id: c.short_id().to_owned(),
            image: c.image.clone(),
            state: c.state.clone(),
            status: c.status.clone(),
            ports,
            project: c.compose.as_ref().map(|p| p.project.clone()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Image {
    /// Its first tag, or `<none>`.
    pub name: String,
    pub id: String,
    pub size: String,
    pub created: String,
    /// Further tags, when it has more than one.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub also: Vec<String>,
}

impl From<&ImageInfo> for Image {
    fn from(i: &ImageInfo) -> Self {
        Self {
            name: i.name().to_owned(),
            id: i.short_id().to_owned(),
            size: format_size(i.size),
            created: time_ago(i.created),
            also: i.tags.iter().skip(1).cloned().collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Volume {
    pub name: String,
    pub driver: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,
}

impl From<&VolumeInfo> for Volume {
    fn from(v: &VolumeInfo) -> Self {
        Self {
            name: v.name.clone(),
            driver: v.driver.clone(),
            created: v.created.map(time_ago),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Network {
    pub name: String,
    pub id: String,
    pub driver: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub subnets: Vec<String>,
    /// No way out to the internet.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub internal: bool,
}

impl From<&NetworkInfo> for Network {
    fn from(n: &NetworkInfo) -> Self {
        Self {
            name: n.name.clone(),
            id: n.short_id().to_owned(),
            driver: n.driver.clone(),
            subnets: n.subnets.clone(),
            internal: n.internal,
        }
    }
}

/// A Docker Compose project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Project {
    pub name: String,
    /// Containers running (or paused), of `containers`.
    pub running: usize,
    pub containers: usize,
    pub services: Vec<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub dir: String,
}

impl From<&ComposeProject> for Project {
    fn from(p: &ComposeProject) -> Self {
        Self {
            name: p.name.clone(),
            running: p.running(),
            containers: p.containers.len(),
            services: p.services().into_iter().map(str::to_owned).collect(),
            dir: p.working_dir.clone(),
        }
    }
}

/// A Docker engine Ferrodock knows of.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Engine {
    pub name: String,
    pub host: String,
    /// The one commands use.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub in_use: bool,
    /// Its Docker version, when it answered.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// A Quick start template.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Template {
    pub name: String,
    pub summary: String,
    /// With its default tag.
    pub image: String,
    pub tags: Vec<String>,
    /// Default host ports.
    pub ports: Vec<u16>,
    /// Settings that can be changed, with their defaults (`KEY=value`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub settings: Vec<String>,
}

impl From<&super::Template> for Template {
    fn from(t: &super::Template) -> Self {
        Self {
            name: t.slug(),
            summary: t.summary.to_owned(),
            image: format!("{}:{}", t.image, t.tags[0]),
            tags: t.tags.iter().map(|s| (*s).to_owned()).collect(),
            ports: t.ports.iter().map(|p| p.host).collect(),
            settings: t
                .fields
                .iter()
                .map(|f| format!("{}={}", f.env, f.default))
                .collect(),
        }
    }
}

/// A service started from a Quick start template.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Started {
    pub name: String,
    pub image: String,
    pub ports: Vec<String>,
    /// How to connect, e.g. `postgres://postgres:postgres@localhost:5432/app`.
    pub connection: String,
}

impl Started {
    pub fn new(template: &super::Template, choices: &super::Choices) -> Self {
        Self {
            name: choices.name.clone(),
            image: template.reference(choices),
            ports: template
                .ports
                .iter()
                .zip(&choices.ports)
                .map(|(p, host)| format!("{host}->{}/tcp", p.container))
                .collect(),
            connection: template.connection(choices),
        }
    }
}

/// Space one kind of thing takes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Space {
    pub size: String,
    pub reclaimable: String,
    pub count: i64,
    pub in_use: i64,
}

impl From<&Usage> for Space {
    fn from(u: &Usage) -> Self {
        Self {
            size: format_size(u.total_size),
            reclaimable: format_size(u.reclaimable),
            count: u.count,
            in_use: u.active,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Disk {
    pub total: String,
    pub reclaimable: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub images: Option<Space>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub containers: Option<Space>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volumes: Option<Space>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build_cache: Option<Space>,
}

impl From<&DiskUsage> for Disk {
    fn from(d: &DiskUsage) -> Self {
        Self {
            total: format_size(d.total_size()),
            reclaimable: format_size(d.reclaimable()),
            images: d.images.as_ref().map(Space::from),
            containers: d.containers.as_ref().map(Space::from),
            volumes: d.volumes.as_ref().map(Space::from),
            build_cache: d.build_cache.as_ref().map(Space::from),
        }
    }
}

/// What people look for in `docker inspect`, without the rest of it.
/// Secret-looking environment values are hidden unless asked for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Details {
    pub id: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub image: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub env: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ports: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub mounts: Vec<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub network: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restart: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub health: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<String>,
}

impl Details {
    pub fn new(i: &Inspection, show_secrets: bool) -> Self {
        let run = &i.run;
        // The inspection's own ID is whatever it was asked by (a name, say);
        // Docker's output has the real one.
        let id = serde_json::from_str::<serde_json::Value>(&i.json)
            .ok()
            .and_then(|v| v["Id"].as_str().map(str::to_owned))
            .unwrap_or_else(|| i.id.clone());
        Self {
            id: id.chars().take(12).collect(),
            image: run.image.clone(),
            env: i
                .env
                .iter()
                .map(|(key, value)| {
                    if show_secrets || !looks_secret(key) {
                        format!("{key}={value}")
                    } else {
                        format!("{key}=(hidden)")
                    }
                })
                .collect(),
            ports: run
                .ports
                .iter()
                .map(|(host, container)| format!("{host}->{container}/tcp"))
                .collect(),
            mounts: run
                .volumes
                .iter()
                .map(|(source, target)| format!("{source}:{target}"))
                .collect(),
            network: run.network.clone(),
            command: run.command.clone(),
            restart: i.restart_policy.clone(),
            health: i.health.as_ref().map(|h| h.status.clone()),
            labels: i
                .labels
                .iter()
                // Compose's own bookkeeping is noise here.
                .filter(|(key, _)| !key.starts_with("com.docker.compose."))
                .map(|(key, value)| format!("{key}={value}"))
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ports_read_like_the_docker_command() {
        let mapping = |ip: &str, public, private| PortMapping {
            ip: ip.into(),
            public,
            private,
            proto: "tcp".into(),
        };
        assert_eq!(port(&mapping("0.0.0.0", Some(8080), 80)), "8080->80/tcp");
        assert_eq!(
            port(&mapping("127.0.0.1", Some(5432), 5432)),
            "127.0.0.1:5432->5432/tcp"
        );
        assert_eq!(port(&mapping("", None, 6379)), "6379/tcp");
        let container = ContainerInfo {
            name: "web".into(),
            id: "0123456789abcdef".into(),
            ports: vec![
                mapping("0.0.0.0", Some(8080), 80),
                mapping("::", Some(8080), 80),
            ],
            ..Default::default()
        };
        let json = serde_json::to_string(&Container::from(&container)).unwrap();
        assert_eq!(
            json,
            r#"{"name":"web","id":"0123456789ab","image":"","state":"","status":"","ports":["8080->80/tcp"]}"#
        );
    }
}
