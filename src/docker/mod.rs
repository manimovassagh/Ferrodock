//! Thin wrapper around the Docker Engine API (via `bollard`).
//!
//! On Windows this talks to Docker Desktop / dockerd over the
//! `\\.\pipe\docker_engine` named pipe; elsewhere it uses the unix socket.
//! `DOCKER_HOST` is honoured on every platform.
//!
//! Each resource type lives in its own file and adds its calls to
//! [`DockerClient`].

mod actions;
mod compose;
mod containers;
mod format;
mod images;
mod logs;
mod networks;
mod stats;
mod volumes;

pub use actions::{Action, Task};
pub use compose::{ComposeProject, group_projects};
pub use containers::ContainerInfo;
pub use format::{format_size, plural, time_ago};
pub use images::ImageInfo;
pub use networks::NetworkInfo;
pub use stats::StatsSample;
pub use volumes::VolumeInfo;

use bollard::Docker;

/// The kinds of things the Docker Engine manages that Ferrodock shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Resource {
    Containers,
    Images,
    Volumes,
    Networks,
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

/// Docker IDs look like `sha256:abc…`; this returns the familiar 12-char form.
fn short_id(id: &str) -> &str {
    let id = id.strip_prefix("sha256:").unwrap_or(id);
    &id[..id.len().min(12)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortens_ids() {
        assert_eq!(short_id("sha256:0123456789abcdef"), "0123456789ab");
        assert_eq!(short_id("abc"), "abc");
    }

    /// Talks to a real daemon: `cargo test -- --ignored`.
    #[tokio::test]
    #[ignore]
    async fn live_daemon_lists_everything() {
        let client = DockerClient::connect().unwrap();
        client.version().await.unwrap();
        client.list_containers().await.unwrap();
        client.list_images().await.unwrap();
        client.list_volumes().await.unwrap();
        client.list_networks().await.unwrap();
    }
}
