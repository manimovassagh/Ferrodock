//! Container logs: the recent tail, then new output as it arrives.

use futures_util::{Stream, StreamExt};

use bollard::query_parameters::LogsOptions;

use super::{DockerClient, describe};

impl DockerClient {
    /// Streams a container's stdout and stderr, starting with the last
    /// `tail` lines. Items are raw chunks: a chunk can hold several lines or
    /// part of one. The stream ends when the container stops.
    pub fn logs(
        &self,
        id: &str,
        tail: usize,
    ) -> impl Stream<Item = Result<String, String>> + use<> {
        let opts = LogsOptions {
            follow: true,
            stdout: true,
            stderr: true,
            tail: tail.to_string(),
            ..Default::default()
        };
        self.docker
            .logs(id, Some(opts))
            .map(|chunk| chunk.map(|output| output.to_string()).map_err(describe))
    }
}
