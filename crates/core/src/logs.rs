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
        // Each line starts with when it was written; the UI can show it.
        self.logs_with(id, tail, true, true)
    }

    /// Like [`DockerClient::logs`], but it can stop after the tail instead
    /// of following, and leave out the timestamps.
    pub fn logs_with(
        &self,
        id: &str,
        tail: usize,
        follow: bool,
        timestamps: bool,
    ) -> impl Stream<Item = Result<String, String>> + use<> {
        let opts = LogsOptions {
            follow,
            stdout: true,
            stderr: true,
            tail: tail.to_string(),
            timestamps,
            ..Default::default()
        };
        self.docker
            .logs(id, Some(opts))
            .map(|chunk| chunk.map(|output| output.to_string()).map_err(describe))
    }
}
