//! Published ports, and the browser URLs they're reachable at.

use std::sync::OnceLock;

use bollard::models::PortSummary;

/// One port of a container, e.g. `0.0.0.0:8080 -> 80/tcp`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PortMapping {
    /// Host address the port is bound to; empty when not published.
    pub ip: String,
    /// Host port, when the port is published.
    pub public: Option<u16>,
    pub private: u16,
    /// `tcp`, `udp` or `sctp`.
    pub proto: String,
}

impl From<&PortSummary> for PortMapping {
    fn from(p: &PortSummary) -> Self {
        Self {
            ip: p.ip.clone().unwrap_or_default(),
            public: p.public_port,
            private: p.private_port,
            proto: p.typ.map(|t| t.to_string()).unwrap_or_else(|| "tcp".into()),
        }
    }
}

impl PortMapping {
    /// Full description, e.g. `0.0.0.0:8080 -> 80/tcp` or `443/tcp`.
    pub fn label(&self) -> String {
        match self.public {
            Some(public) => {
                let ip = if self.ip.is_empty() {
                    "0.0.0.0"
                } else {
                    &self.ip
                };
                format!("{ip}:{public} -> {}/{}", self.private, self.proto)
            }
            None => format!("{}/{}", self.private, self.proto),
        }
    }

    /// Short form for tight spaces, e.g. `8080 → 80`.
    pub fn short_label(&self) -> String {
        match self.public {
            Some(public) => format!("{public} → {}", self.private),
            None => format!("{}/{}", self.private, self.proto),
        }
    }

    /// The URL a browser can open this port at, for published TCP ports.
    /// `engine_host` is where the Docker Engine runs (see [`engine_host`]).
    pub fn url(&self, engine_host: &str) -> Option<String> {
        let public = self.public.filter(|_| self.proto == "tcp")?;
        let host = match self.ip.as_str() {
            "" | "0.0.0.0" | "::" => engine_host.to_owned(),
            // On a remote engine a specific address is only meaningful on
            // the engine's own host.
            _ if engine_host != "localhost" => engine_host.to_owned(),
            ip if ip.contains(':') => format!("[{ip}]"),
            ip => ip.to_owned(),
        };
        let scheme = if matches!(self.private, 443 | 8443) || matches!(public, 443 | 8443) {
            "https"
        } else {
            "http"
        };
        Some(format!("{scheme}://{host}:{public}"))
    }
}

/// The host published ports are reachable on: the host part of a `tcp://`
/// or `ssh://` `DOCKER_HOST`, otherwise `localhost`.
pub fn engine_host() -> &'static str {
    static HOST: OnceLock<String> = OnceLock::new();
    HOST.get_or_init(|| host_from_docker_host(std::env::var("DOCKER_HOST").ok().as_deref()))
}

fn host_from_docker_host(docker_host: Option<&str>) -> String {
    let remote = docker_host.and_then(|h| {
        let rest = h
            .strip_prefix("tcp://")
            .or_else(|| h.strip_prefix("ssh://"))
            .or_else(|| h.strip_prefix("http://"))
            .or_else(|| h.strip_prefix("https://"))?;
        let authority = rest.split('/').next().unwrap_or(rest);
        let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
        let host = match host.strip_prefix('[') {
            Some(v6) => format!("[{}]", v6.split(']').next().unwrap_or(v6)),
            None => host.split(':').next().unwrap_or(host).to_owned(),
        };
        (!host.is_empty()).then_some(host)
    });
    remote.unwrap_or_else(|| "localhost".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn port(ip: &str, public: Option<u16>, private: u16, proto: &str) -> PortMapping {
        PortMapping {
            ip: ip.into(),
            public,
            private,
            proto: proto.into(),
        }
    }

    #[test]
    fn labels() {
        assert_eq!(
            port("0.0.0.0", Some(8080), 80, "tcp").label(),
            "0.0.0.0:8080 -> 80/tcp"
        );
        assert_eq!(port("", None, 443, "tcp").label(), "443/tcp");
        assert_eq!(port("::", Some(8080), 80, "tcp").short_label(), "8080 → 80");
    }

    #[test]
    fn urls() {
        assert_eq!(
            port("0.0.0.0", Some(8080), 80, "tcp")
                .url("localhost")
                .as_deref(),
            Some("http://localhost:8080")
        );
        assert_eq!(
            port("::", Some(8443), 8443, "tcp")
                .url("localhost")
                .as_deref(),
            Some("https://localhost:8443")
        );
        assert_eq!(
            port("127.0.0.1", Some(5432), 5432, "tcp")
                .url("localhost")
                .as_deref(),
            Some("http://127.0.0.1:5432")
        );
        assert_eq!(
            port("0.0.0.0", Some(3000), 3000, "tcp")
                .url("devbox")
                .as_deref(),
            Some("http://devbox:3000")
        );
        assert_eq!(port("0.0.0.0", Some(53), 53, "udp").url("localhost"), None);
        assert_eq!(port("", None, 80, "tcp").url("localhost"), None);
    }

    #[test]
    fn engine_hosts() {
        assert_eq!(host_from_docker_host(None), "localhost");
        assert_eq!(
            host_from_docker_host(Some("unix:///var/run/docker.sock")),
            "localhost"
        );
        assert_eq!(
            host_from_docker_host(Some("tcp://10.0.0.5:2376")),
            "10.0.0.5"
        );
        assert_eq!(host_from_docker_host(Some("ssh://me@devbox")), "devbox");
        assert_eq!(
            host_from_docker_host(Some("tcp://[fd00::1]:2375")),
            "[fd00::1]"
        );
    }
}
