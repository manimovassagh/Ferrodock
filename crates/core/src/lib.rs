//! `ferrodock-core`: Ferrodock's Docker layer, shared by the desktop app,
//! the `fdock` CLI and the MCP server. A thin wrapper around the Docker
//! Engine API (via `bollard`), with no UI code.
//!
//! On Windows this talks to the Docker engine over the
//! `\\.\pipe\docker_engine` named pipe; elsewhere it uses the unix socket.
//! `DOCKER_HOST` is honoured on every platform.
//!
//! Each resource type lives in its own file and adds its calls to
//! [`DockerClient`].

mod actions;
mod browse;
pub mod builtin;
mod compose;
pub mod compose_cli;
pub mod compose_install;
pub mod connect;
mod containers;
mod create;
mod debug;
mod disk;
mod engines;
mod events;
mod files;
mod format;
mod images;
mod inspect;
mod launch;
mod limits;
mod logs;
mod networks;
mod ports;
mod processes;
mod registry;
mod reset;
mod run;
mod stats;
pub mod summary;
mod templates;
mod terminal;
mod volume_backup;
mod volumes;

pub use actions::{Action, Task};
pub use browse::VOLUME_ROOT;
pub use compose::{ComposeProject, group_projects};
pub use compose_cli::{ComposeAction, ComposeOp, ComposeTarget, next_job, split_log_line};
pub use containers::{ComposeLabels, ContainerInfo};
pub use create::Change;
pub use debug::{TARGET_ROOT, debug_image};
pub use disk::{CLEAN_TARGETS, DiskUsage, PruneKind, Usage, clean_kinds};
pub use engines::{Engine, builtin_engine, docker_dir, engines, pick};
pub use events::EngineEvent;
pub use files::{
    FileEntry, FileKind, FileOp, Listing, Preview, downloads_dir, join_path, parent_path,
};
pub use format::{format_size, plural, time_ago};
pub use images::ImageInfo;
pub use inspect::{Health, Inspection, RunSettings, looks_secret};
pub use launch::{Starter, ferrodock_app, provider_app, starter, starter_outside_app};
pub use limits::{cpus_text, memory_text, parse_cpus, parse_memory};
pub use networks::NetworkInfo;
pub use ports::{PortMapping, engine_host};
pub use processes::{Layer, Processes};
pub use registry::{ImageOp, SearchResult};
pub use reset::RESET_WORD;
pub use run::{PullProgress, Restart, RunSpec};
pub use stats::StatsSample;
pub use templates::{Choices, Overrides, TEMPLATES, Template, port_free, template};
pub use terminal::{TermOp, exit_message, next_session};
pub use volumes::VolumeInfo;

use bollard::Docker;

/// The kinds of things the Docker Engine manages that Ferrodock shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Resource {
    Containers,
    Images,
    Volumes,
    Networks,
    /// Disk usage (`docker system df`). Loaded on request, never polled,
    /// because it can be slow on big hosts.
    Disk,
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

    /// A client for `engine`. Like [`DockerClient::connect`], this doesn't
    /// touch the engine yet.
    pub fn connect_to(engine: &Engine) -> Result<Self, String> {
        engine.connect().map(|docker| Self { docker })
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
            format!("Cannot reach the Docker Engine ({err}). Is the Docker engine running?")
        }
        other => other.to_string(),
    }
}

/// `bytes` as lowercase hex, the way `sha256sum` prints a digest.
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
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

    #[test]
    fn hex_is_lowercase_and_padded() {
        assert_eq!(hex(&[0x00, 0x0f, 0xab]), "000fab");
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
