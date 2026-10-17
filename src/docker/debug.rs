//! Debug shells: a shell next to a container that has none of its own
//! (distroless, scratch). A helper container shares the target's process
//! and network namespaces, so `ps`, `netstat` and `wget localhost` see the
//! target, and its filesystem is at [`TARGET_ROOT`]. Helpers carry
//! [`DEBUG_LABEL`], are hidden from the container list, and are removed when
//! the session ends.

use std::collections::HashMap;

use bollard::models::{ContainerCreateBody, HostConfig};
use bollard::query_parameters::{
    ListContainersOptions, RemoveContainerOptions, StartContainerOptions,
};

use super::{DockerClient, describe};

/// Marks debug helpers; the value is the target container's ID.
pub const DEBUG_LABEL: &str = "dev.ferrodock.debug-shell";
/// Where the target's filesystem shows up in the helper.
pub const TARGET_ROOT: &str = "/target";
/// A small image with a shell and the usual tools.
pub const DEBUG_IMAGE: &str = "busybox:latest";

/// The image debug helpers use: [`DEBUG_IMAGE`], unless the
/// `FERRODOCK_DEBUG_IMAGE` environment variable names another (say, a
/// mirror for machines that can't reach Docker Hub).
pub fn debug_image() -> String {
    std::env::var("FERRODOCK_DEBUG_IMAGE")
        .ok()
        .filter(|i| !i.trim().is_empty())
        .unwrap_or_else(|| DEBUG_IMAGE.to_owned())
}

impl DockerClient {
    /// Starts a helper next to `target`, made from `image` (pulled if it
    /// isn't there). Returns its ID.
    pub async fn start_debug_helper(&self, target: &str, image: &str) -> Result<String, String> {
        let local = self.list_images().await?;
        let have = local
            .iter()
            .any(|i| i.id == image || i.tags.iter().any(|t| t == image));
        if !have {
            self.pull(image, |_| {})
                .await
                .map_err(|e| format!("couldn't get {image} for the debug shell: {e}"))?;
        }
        let ns = format!("container:{target}");
        // The target's root is visible through its PID 1, which the shared
        // PID namespace makes ours too.
        let script = format!("ln -sfn /proc/1/root {TARGET_ROOT}; exec sleep 86400");
        let body = ContainerCreateBody {
            image: Some(image.to_owned()),
            entrypoint: Some(vec!["sh".into(), "-c".into(), script]),
            cmd: Some(Vec::new()),
            labels: Some(HashMap::from([(DEBUG_LABEL.to_owned(), target.to_owned())])),
            host_config: Some(HostConfig {
                pid_mode: Some(ns.clone()),
                network_mode: Some(ns),
                // Reading another process's root needs ptrace access.
                cap_add: Some(vec!["SYS_PTRACE".into()]),
                auto_remove: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        };
        let created = self
            .docker
            .create_container(
                None::<bollard::query_parameters::CreateContainerOptions>,
                body,
            )
            .await
            .map_err(describe)?;
        if let Err(e) = self
            .docker
            .start_container(&created.id, None::<StartContainerOptions>)
            .await
        {
            let _ = self.remove_debug_helper(&created.id).await;
            return Err(describe(e));
        }
        Ok(created.id)
    }

    pub async fn remove_debug_helper(&self, id: &str) -> Result<(), String> {
        let opts = RemoveContainerOptions {
            force: true,
            ..Default::default()
        };
        self.docker
            .remove_container(id, Some(opts))
            .await
            .map_err(describe)
    }

    /// Removes debug helpers left from an earlier run.
    pub async fn remove_debug_helpers(&self) -> Result<(), String> {
        let opts = ListContainersOptions {
            all: true,
            filters: Some(HashMap::from([(
                "label".to_owned(),
                vec![DEBUG_LABEL.to_owned()],
            )])),
            ..Default::default()
        };
        let list = self
            .docker
            .list_containers(Some(opts))
            .await
            .map_err(describe)?;
        for id in list.into_iter().filter_map(|c| c.id) {
            self.remove_debug_helper(&id).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Talks to a real daemon: `cargo test -- --ignored`. Debugs a running
    /// container through a helper (made from a local image, since pulling
    /// may be rate-limited): the helper sees the target's processes and
    /// files, is hidden, and goes away when removed.
    #[tokio::test]
    #[ignore]
    async fn live_daemon_debugs_a_container() {
        use futures_util::StreamExt;
        use tokio::io::AsyncWriteExt;

        let client = DockerClient::connect().unwrap();
        let Some(image) = client.list_images().await.unwrap().into_iter().next() else {
            return;
        };
        let spec = crate::docker::RunSpec {
            image: image.id.clone(),
            name: "ferrodock-debug-target".into(),
            command: "sleep 4321".into(),
            ..Default::default()
        };
        let opts = || RemoveContainerOptions {
            force: true,
            ..Default::default()
        };
        let _ = client
            .docker
            .remove_container(&spec.name, Some(opts()))
            .await;
        let target = client.run(&spec).await.unwrap();
        let helper = client.start_debug_helper(&target, &image.id).await.unwrap();

        let mut sh = client.open_shell(&helper, "", 120, 30).await.unwrap();
        sh.input
            .write_all(b"ps; ls /target/; echo done-$((1+1))\r")
            .await
            .unwrap();
        let mut screen = vt100::Parser::new(30, 120, 0);
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
        while !screen.screen().contents().contains("done-2") {
            match tokio::time::timeout_at(deadline, sh.output.next()).await {
                Ok(Some(Ok(out))) => screen.process(&out.into_bytes()),
                _ => break,
            }
        }
        let shown = screen.screen().contents();
        let hidden = client
            .list_containers()
            .await
            .unwrap()
            .iter()
            .all(|c| c.id != helper);
        client.remove_debug_helper(&helper).await.unwrap();
        client
            .docker
            .remove_container(&target, Some(opts()))
            .await
            .unwrap();
        assert!(
            shown.contains("sleep 4321"),
            "sees the target's processes:\n{shown}"
        );
        assert!(shown.contains("etc"), "sees the target's files:\n{shown}");
        assert!(hidden, "helpers don't show in the container list");
    }
}
