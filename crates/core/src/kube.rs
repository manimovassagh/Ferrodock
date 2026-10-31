//! Kubernetes clusters, made only when asked: k3s in containers, a server
//! and any number of workers on a network of their own. Nothing runs until a
//! cluster is created, and a stopped cluster uses nothing but disk.
//! Every container of a cluster carries [`KUBE_LABEL`].

use std::collections::HashMap;

use bollard::exec::{CreateExecOptions, StartExecResults};
use bollard::models::{ContainerCreateBody, HostConfig, NetworkCreateRequest, PortBinding};
use bollard::query_parameters::{
    CreateContainerOptionsBuilder, ListContainersOptions, RemoveContainerOptions,
    RemoveVolumeOptions, StartContainerOptions, StopContainerOptions, UploadToContainerOptions,
};
use futures_util::StreamExt;
use serde_json::Value;

use super::{DockerClient, describe};

/// Marks a cluster's containers, network and volume; the value is its name.
pub const KUBE_LABEL: &str = "dev.ferrodock.kube";
/// `server` or `worker`.
pub const ROLE_LABEL: &str = "dev.ferrodock.kube.role";
/// The port of this computer the Kubernetes API is on.
pub const API_LABEL: &str = "dev.ferrodock.kube.api";
/// The port of this computer the cluster's ingress (port 80) is on.
pub const INGRESS_LABEL: &str = "dev.ferrodock.kube.ingress";
/// The kubectl context Ferrodock added for it; empty for none.
pub const CONTEXT_LABEL: &str = "dev.ferrodock.kube.context";

/// The k3s image, without a tag.
pub const K3S_IMAGE: &str = "rancher/k3s";
/// Kubernetes versions offered, newest (the default) first: the minor
/// version shown, and the k3s tag.
pub const KUBE_VERSIONS: &[(&str, &str)] = &[
    ("1.37", "v1.37.1-k3s1"),
    ("1.36", "v1.36.5-k3s1"),
    ("1.35", "v1.35.9-k3s1"),
];
/// What every node's kubelet is started with.
const KUBELET_ARGS: &[&str] = &[
    // Kubernetes 1.35+ won't run on cgroup v1 unless told to; harmless on
    // v2.
    "--kubelet-arg=fail-cgroupv1=false",
    // Only evict pods (and delete images) when the disk is really full: a
    // laptop's disk is often 85% full, which would stop pods by default.
    "--kubelet-arg=eviction-hard=imagefs.available<1%,nodefs.available<1%",
    "--kubelet-arg=eviction-minimum-reclaim=imagefs.available=1%,nodefs.available=1%",
];
/// Where images to load are put, in a volume every node shares.
const IMAGES_DIR: &str = "/images";

/// Something to do with a cluster, done by the app's worker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KubeOp {
    Create(Box<ClusterSpec>),
    Start(String),
    Stop(String),
    /// The cluster, and the kubectl context to take out (empty for none).
    Remove {
        cluster: String,
        context: String,
    },
    /// Read its nodes and pods.
    Status(String),
    /// Copy an image from Docker into the cluster.
    LoadImage {
        cluster: String,
        image: String,
    },
}

/// Everything to create a cluster.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClusterSpec {
    pub name: String,
    /// The k3s tag, e.g. `v1.37.1-k3s1`.
    pub tag: String,
    /// Worker nodes besides the server, which runs workloads too.
    pub workers: u8,
    pub api_port: u16,
    pub ingress_port: u16,
    /// Add a context for it to the kubeconfig, and switch to it.
    pub kubectl: bool,
}

/// The names of a cluster's parts.
pub fn server_name(cluster: &str) -> String {
    format!("{cluster}-server")
}

pub fn worker_name(cluster: &str, n: u8) -> String {
    format!("{cluster}-worker-{n}")
}

fn network_name(cluster: &str) -> String {
    format!("ferrodock-kube-{cluster}")
}

fn volume_name(cluster: &str) -> String {
    format!("ferrodock-kube-{cluster}-images")
}

/// The kubectl context of a cluster.
pub fn context_name(cluster: &str) -> String {
    format!("ferrodock-{cluster}")
}

/// A secret the nodes of one cluster share to join it.
fn token() -> String {
    use std::hash::{BuildHasher, Hasher};
    let part = || {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u128(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos()),
        );
        h.finish()
    };
    format!("{:016x}{:016x}", part(), part())
}

impl ClusterSpec {
    pub fn image(&self) -> String {
        format!("{K3S_IMAGE}:{}", self.tag)
    }

    /// The Kubernetes version, e.g. `1.37`.
    pub fn version(&self) -> &str {
        KUBE_VERSIONS
            .iter()
            .find(|(_, tag)| *tag == self.tag)
            .map_or(self.tag.as_str(), |(v, _)| v)
    }

    /// Where the API is, from this computer.
    pub fn api_url(&self) -> String {
        format!("https://{}:{}", api_host(), self.api_port)
    }

    fn labels(&self, role: &str) -> HashMap<String, String> {
        HashMap::from([
            (KUBE_LABEL.to_owned(), self.name.clone()),
            (ROLE_LABEL.to_owned(), role.to_owned()),
            (API_LABEL.to_owned(), self.api_port.to_string()),
            (INGRESS_LABEL.to_owned(), self.ingress_port.to_string()),
            (
                CONTEXT_LABEL.to_owned(),
                if self.kubectl {
                    context_name(&self.name)
                } else {
                    String::new()
                },
            ),
        ])
    }

    /// What every node has: privileges k3s needs, its network, the shared
    /// images folder.
    fn node(
        &self,
        role: &str,
        hostname: String,
        cmd: Vec<String>,
        env: Vec<String>,
    ) -> ContainerCreateBody {
        ContainerCreateBody {
            image: Some(self.image()),
            hostname: Some(hostname),
            cmd: Some(cmd),
            env: Some(env),
            labels: Some(self.labels(role)),
            host_config: Some(HostConfig {
                privileged: Some(true),
                tmpfs: Some(HashMap::from([
                    ("/run".to_owned(), String::new()),
                    ("/var/run".to_owned(), String::new()),
                ])),
                network_mode: Some(network_name(&self.name)),
                binds: Some(vec![format!("{}:{IMAGES_DIR}", volume_name(&self.name))]),
                ..HostConfig::default()
            }),
            ..ContainerCreateBody::default()
        }
    }

    fn server(&self, token: &str) -> ContainerCreateBody {
        let host = api_host();
        let mut cmd = vec!["server".to_owned()];
        for san in ["127.0.0.1", "localhost", host] {
            cmd.push(format!("--tls-san={san}"));
        }
        cmd.extend(KUBELET_ARGS.iter().map(|a| (*a).to_owned()));
        let mut body = self.node(
            "server",
            server_name(&self.name),
            cmd,
            vec![format!("K3S_TOKEN={token}")],
        );
        // On all of the engine's addresses, like any published port: the
        // Mac engine's VM forwards only those to the Mac's localhost.
        let bind = |port: u16| {
            Some(vec![PortBinding {
                host_ip: None,
                host_port: Some(port.to_string()),
            }])
        };
        let ports = HashMap::from([
            ("6443/tcp".to_owned(), bind(self.api_port)),
            ("80/tcp".to_owned(), bind(self.ingress_port)),
        ]);
        body.exposed_ports = Some(vec!["6443/tcp".into(), "80/tcp".into()]);
        if let Some(host) = body.host_config.as_mut() {
            host.port_bindings = Some(ports);
        }
        body
    }

    fn worker(&self, n: u8, token: &str) -> ContainerCreateBody {
        self.node(
            "worker",
            worker_name(&self.name, n),
            std::iter::once("agent")
                .chain(KUBELET_ARGS.iter().copied())
                .map(str::to_owned)
                .collect(),
            vec![
                format!("K3S_URL=https://{}:6443", server_name(&self.name)),
                format!("K3S_TOKEN={token}"),
            ],
        )
    }
}

/// The host the cluster's API and ingress are reached on.
fn api_host() -> &'static str {
    let host = super::engine_host();
    if host == "localhost" {
        "127.0.0.1"
    } else {
        host
    }
}

/// A node, as `kubectl get nodes` shows it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct KubeNode {
    pub name: String,
    pub ready: bool,
    /// `control-plane`, or empty for a worker.
    pub roles: String,
    pub version: String,
}

/// A pod, as `kubectl get pods` shows it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct KubePod {
    pub namespace: String,
    pub name: String,
    /// Ready containers out of all, e.g. `1/1`.
    pub ready: String,
    /// `Running`, `Completed`, or why it isn't, e.g. `ImagePullBackOff`.
    pub status: String,
    pub restarts: i64,
    pub node: String,
}

impl KubePod {
    /// Running with every container ready, or done.
    pub fn healthy(&self) -> bool {
        self.status == "Completed"
            || (self.status == "Running" && self.ready.split_once('/').is_some_and(|(a, b)| a == b))
    }
}

/// What's running in a cluster.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ClusterStatus {
    pub nodes: Vec<KubeNode>,
    pub pods: Vec<KubePod>,
}

fn parse_nodes(json: &str) -> Result<Vec<KubeNode>, String> {
    let list: Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let mut nodes: Vec<KubeNode> = items(&list)
        .iter()
        .map(|n| {
            let mut roles: Vec<&str> = n["metadata"]["labels"]
                .as_object()
                .map(|labels| {
                    labels
                        .keys()
                        .filter_map(|k| k.strip_prefix("node-role.kubernetes.io/"))
                        .collect()
                })
                .unwrap_or_default();
            roles.sort();
            KubeNode {
                name: text(&n["metadata"]["name"]),
                ready: n["status"]["conditions"].as_array().is_some_and(|c| {
                    c.iter()
                        .any(|c| c["type"] == "Ready" && c["status"] == "True")
                }),
                roles: roles.join(","),
                version: text(&n["status"]["nodeInfo"]["kubeletVersion"]),
            }
        })
        .collect();
    nodes.sort_by(|a, b| (a.roles.is_empty(), &a.name).cmp(&(b.roles.is_empty(), &b.name)));
    Ok(nodes)
}

fn parse_pods(json: &str) -> Result<Vec<KubePod>, String> {
    let list: Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let mut pods: Vec<KubePod> = items(&list)
        .iter()
        .map(|p| {
            let statuses = p["status"]["containerStatuses"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            let total = p["spec"]["containers"]
                .as_array()
                .map_or(statuses.len(), Vec::len);
            let ready = statuses.iter().filter(|s| s["ready"] == true).count();
            let restarts = statuses
                .iter()
                .filter_map(|s| s["restartCount"].as_i64())
                .sum();
            // Like kubectl: why a container waits or ended beats the phase.
            let reason = statuses.iter().find_map(|s| {
                s["state"]["waiting"]["reason"]
                    .as_str()
                    .or_else(|| s["state"]["terminated"]["reason"].as_str())
            });
            let phase = text(&p["status"]["phase"]);
            let status = if p["metadata"]["deletionTimestamp"].is_string() {
                "Terminating".to_owned()
            } else if phase == "Succeeded" {
                "Completed".to_owned()
            } else {
                reason.map_or(phase, str::to_owned)
            };
            KubePod {
                namespace: text(&p["metadata"]["namespace"]),
                name: text(&p["metadata"]["name"]),
                ready: format!("{ready}/{total}"),
                status,
                restarts,
                node: text(&p["spec"]["nodeName"]),
            }
        })
        .collect();
    // Your own workloads first, then the system's.
    pods.sort_by(|a, b| {
        (a.namespace == "kube-system", &a.namespace, &a.name).cmp(&(
            b.namespace == "kube-system",
            &b.namespace,
            &b.name,
        ))
    });
    Ok(pods)
}

fn items(list: &Value) -> Vec<Value> {
    list["items"].as_array().cloned().unwrap_or_default()
}

fn text(v: &Value) -> String {
    v.as_str().unwrap_or_default().to_owned()
}

/// A tar holding one file, `name`, of `size` bytes, streamed from `file`.
fn tar_stream(
    name: &str,
    size: u64,
    file: tokio::fs::File,
) -> Result<
    impl futures_util::Stream<Item = Result<bytes::Bytes, std::io::Error>> + Send + 'static,
    String,
> {
    let mut header = tar::Header::new_gnu();
    header.set_path(name).map_err(|e| e.to_string())?;
    header.set_size(size);
    header.set_mode(0o644);
    header.set_cksum();
    let head = bytes::Bytes::copy_from_slice(header.as_bytes());
    // The file is padded to whole 512-byte blocks; two empty blocks end it.
    let padding = (512 - size % 512) % 512;
    let tail = bytes::Bytes::from(vec![0u8; padding as usize + 1024]);
    let body = futures_util::stream::unfold(file, |mut file| async move {
        use tokio::io::AsyncReadExt;
        let mut buffer = vec![0; 1 << 20];
        match file.read(&mut buffer).await {
            Ok(0) => None,
            Ok(n) => {
                buffer.truncate(n);
                Some((Ok(bytes::Bytes::from(buffer)), file))
            }
            Err(e) => Some((Err(e), file)),
        }
    });
    Ok(futures_util::stream::once(async { Ok(head) })
        .chain(body)
        .chain(futures_util::stream::once(async { Ok(tail) })))
}

impl DockerClient {
    /// Creates and starts a cluster (its image must be here), waiting until
    /// Kubernetes answers. `step` hears what it's doing. Returns the
    /// kubeconfig k3s wrote. A cluster that can't be made is removed again.
    pub async fn create_cluster(
        &self,
        spec: &ClusterSpec,
        mut step: impl FnMut(&str),
    ) -> Result<String, String> {
        let made = async {
            step("Starting the server…");
            self.docker
                .create_network(NetworkCreateRequest {
                    name: network_name(&spec.name),
                    labels: Some(HashMap::from([(KUBE_LABEL.to_owned(), spec.name.clone())])),
                    ..Default::default()
                })
                .await
                .map_err(describe)?;
            let token = token();
            self.start_node(&server_name(&spec.name), spec.server(&token))
                .await?;
            step("Waiting for Kubernetes…");
            self.wait_ready(&server_name(&spec.name)).await?;
            for n in 1..=spec.workers {
                step(&format!("Starting worker {n} of {}…", spec.workers));
                self.start_node(&worker_name(&spec.name, n), spec.worker(n, &token))
                    .await?;
            }
            if spec.workers > 0 {
                step("Waiting for the workers…");
                self.wait_nodes(&server_name(&spec.name), usize::from(spec.workers) + 1)
                    .await?;
            }
            let (code, kubeconfig, _) = self
                .kube_exec(
                    &server_name(&spec.name),
                    &["cat", "/etc/rancher/k3s/k3s.yaml"],
                )
                .await?;
            if code != Some(0) {
                return Err("k3s didn't write a kubeconfig".into());
            }
            Ok(kubeconfig)
        };
        let result = made.await;
        if result.is_err() {
            let _ = self.remove_cluster(&spec.name).await;
        }
        result
    }

    async fn start_node(&self, name: &str, body: ContainerCreateBody) -> Result<(), String> {
        let options = CreateContainerOptionsBuilder::new().name(name).build();
        let created = self
            .docker
            .create_container(Some(options), body)
            .await
            .map_err(describe)?;
        self.docker
            .start_container(&created.id, None::<StartContainerOptions>)
            .await
            .map_err(describe)
    }

    /// Waits up to three minutes for the API server to say it's ready.
    async fn wait_ready(&self, server: &str) -> Result<(), String> {
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(180);
        loop {
            let ready = self
                .kube_exec(server, &["kubectl", "get", "--raw=/readyz"])
                .await;
            match ready {
                Ok((Some(0), out, _)) if out.trim() == "ok" => return Ok(()),
                _ if tokio::time::Instant::now() > deadline => {
                    let logs = self.last_log_lines(server).await;
                    return Err(format!("Kubernetes didn't start within 3 minutes. {logs}"));
                }
                // Exited (or not up yet): no point waiting if it's gone.
                Err(e) if e.contains("is not running") => {
                    let logs = self.last_log_lines(server).await;
                    return Err(format!("k3s stopped. {logs}"));
                }
                _ => tokio::time::sleep(std::time::Duration::from_secs(2)).await,
            }
        }
    }

    /// Waits up to three minutes for `count` nodes to be ready.
    async fn wait_nodes(&self, server: &str, count: usize) -> Result<(), String> {
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(180);
        loop {
            let ready = match self
                .kube_exec(server, &["kubectl", "get", "nodes", "-o", "json"])
                .await?
            {
                (Some(0), out, _) => parse_nodes(&out)?.iter().filter(|n| n.ready).count(),
                _ => 0,
            };
            if ready >= count {
                return Ok(());
            }
            if tokio::time::Instant::now() > deadline {
                return Err(format!(
                    "only {ready} of {count} nodes were ready after 3 minutes"
                ));
            }
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
    }

    /// k3s's last error, to explain a failed start.
    async fn last_log_lines(&self, container: &str) -> String {
        let chunks: Vec<String> = self
            .logs_with(container, 60, false, false)
            .filter_map(|c| async move { c.ok() })
            .collect()
            .await;
        let text = chunks.concat();
        text.lines()
            .rev()
            .find(|l| l.contains("level=fatal") || l.contains("Error:"))
            .map(|l| l.trim().to_owned())
            .unwrap_or_default()
    }

    /// Runs `cmd` in `container`: its exit code, output and errors.
    async fn kube_exec(
        &self,
        container: &str,
        cmd: &[&str],
    ) -> Result<(Option<i64>, String, String), String> {
        let exec = self
            .docker
            .create_exec(
                container,
                CreateExecOptions::<String> {
                    cmd: Some(cmd.iter().map(|s| (*s).to_owned()).collect()),
                    attach_stdout: Some(true),
                    attach_stderr: Some(true),
                    ..Default::default()
                },
            )
            .await
            .map_err(describe)?;
        let StartExecResults::Attached { mut output, .. } = self
            .docker
            .start_exec(&exec.id, None)
            .await
            .map_err(describe)?
        else {
            return Err("the command didn't attach".into());
        };
        let (mut out, mut err) = (String::new(), String::new());
        while let Some(chunk) = output.next().await {
            match chunk.map_err(describe)? {
                bollard::container::LogOutput::StdOut { message } => {
                    out.push_str(&String::from_utf8_lossy(&message));
                }
                bollard::container::LogOutput::StdErr { message } => {
                    err.push_str(&String::from_utf8_lossy(&message));
                }
                _ => {}
            }
        }
        let code = self
            .docker
            .inspect_exec(&exec.id)
            .await
            .map_err(describe)?
            .exit_code;
        Ok((code, out, err))
    }

    /// The IDs of a cluster's containers: the server first.
    async fn cluster_nodes(&self, cluster: &str) -> Result<Vec<String>, String> {
        let options = ListContainersOptions {
            all: true,
            filters: Some(HashMap::from([(
                "label".to_owned(),
                vec![format!("{KUBE_LABEL}={cluster}")],
            )])),
            ..Default::default()
        };
        let mut nodes: Vec<(bool, String, String)> = self
            .docker
            .list_containers(Some(options))
            .await
            .map_err(describe)?
            .into_iter()
            .map(|c| {
                let labels = c.labels.unwrap_or_default();
                let server = labels.get(ROLE_LABEL).is_some_and(|r| r == "server");
                let name = c
                    .names
                    .and_then(|n| n.into_iter().next())
                    .unwrap_or_default();
                (!server, name, c.id.unwrap_or_default())
            })
            .collect();
        nodes.sort();
        Ok(nodes.into_iter().map(|(_, _, id)| id).collect())
    }

    /// Starts a stopped cluster: the server, then the workers.
    pub async fn start_cluster(&self, cluster: &str) -> Result<(), String> {
        for id in self.cluster_nodes(cluster).await? {
            self.docker
                .start_container(&id, None::<StartContainerOptions>)
                .await
                .or_else(|e| match e {
                    // Already running.
                    bollard::errors::Error::DockerResponseServerError {
                        status_code: 304, ..
                    } => Ok(()),
                    e => Err(describe(e)),
                })?;
        }
        Ok(())
    }

    /// Stops a cluster: the workers, then the server.
    pub async fn stop_cluster(&self, cluster: &str) -> Result<(), String> {
        for id in self.cluster_nodes(cluster).await?.iter().rev() {
            self.docker
                .stop_container(id, None::<StopContainerOptions>)
                .await
                .or_else(|e| match e {
                    bollard::errors::Error::DockerResponseServerError {
                        status_code: 304, ..
                    } => Ok(()),
                    e => Err(describe(e)),
                })?;
        }
        Ok(())
    }

    /// Removes a cluster's containers, network and images volume.
    pub async fn remove_cluster(&self, cluster: &str) -> Result<(), String> {
        for id in self.cluster_nodes(cluster).await? {
            self.docker
                .remove_container(
                    &id,
                    Some(RemoveContainerOptions {
                        force: true,
                        v: true,
                        ..Default::default()
                    }),
                )
                .await
                .map_err(describe)?;
        }
        let _ = self.docker.remove_network(&network_name(cluster)).await;
        let _ = self
            .docker
            .remove_volume(&volume_name(cluster), None::<RemoveVolumeOptions>)
            .await;
        Ok(())
    }

    /// The nodes and pods of a running cluster.
    pub async fn cluster_status(&self, cluster: &str) -> Result<ClusterStatus, String> {
        let server = server_name(cluster);
        let get = |what: &'static str| {
            let server = server.clone();
            async move {
                match self
                    .kube_exec(&server, &["kubectl", "get", what, "-A", "-o", "json"])
                    .await?
                {
                    (Some(0), out, _) => Ok(out),
                    (_, _, err) => Err(err.trim().to_owned()),
                }
            }
        };
        let (nodes, pods) = futures_util::join!(get("nodes"), get("pods"));
        Ok(ClusterStatus {
            nodes: parse_nodes(&nodes?)?,
            pods: parse_pods(&pods?)?,
        })
    }

    /// Copies `image` from Docker into every node of the cluster, so pods
    /// can use it without a registry.
    pub async fn load_into_cluster(&self, cluster: &str, image: &str) -> Result<(), String> {
        let dir = std::env::temp_dir();
        let saved = self.save_image(image, &dir).await?;
        let result = async {
            let size = std::fs::metadata(&saved).map_err(|e| e.to_string())?.len();
            let file = tokio::fs::File::open(&saved)
                .await
                .map_err(|e| e.to_string())?;
            let name = format!("ferrodock-{}.tar", std::process::id());
            let body = tar_stream(&name, size, file)?;
            let server = server_name(cluster);
            self.docker
                .upload_to_container(
                    &server,
                    Some(UploadToContainerOptions {
                        path: IMAGES_DIR.to_owned(),
                        ..Default::default()
                    }),
                    bollard::body_try_stream(body),
                )
                .await
                .map_err(describe)?;
            let path = format!("{IMAGES_DIR}/{name}");
            let mut result = Ok(());
            for id in self.cluster_nodes(cluster).await? {
                if let (code, _, err) = self
                    .kube_exec(&id, &["ctr", "images", "import", &path])
                    .await?
                    && code != Some(0)
                {
                    result = Err(format!("a node couldn't take it: {}", err.trim()));
                    break;
                }
            }
            let _ = self.kube_exec(&server, &["rm", "-f", &path]).await;
            result
        }
        .await;
        let _ = std::fs::remove_file(saved);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> ClusterSpec {
        ClusterSpec {
            name: "dev".into(),
            tag: KUBE_VERSIONS[0].1.into(),
            workers: 2,
            api_port: 6550,
            ingress_port: 8080,
            kubectl: true,
        }
    }

    #[test]
    fn builds_the_nodes() {
        let spec = spec();
        let server = spec.server("t0k3n");
        assert_eq!(server.image.as_deref(), Some("rancher/k3s:v1.37.1-k3s1"));
        assert_eq!(server.hostname.as_deref(), Some("dev-server"));
        let cmd = server.cmd.unwrap();
        assert_eq!(cmd[0], "server");
        assert!(cmd.contains(&"--kubelet-arg=fail-cgroupv1=false".to_owned()));
        let labels = server.labels.unwrap();
        assert_eq!(labels[KUBE_LABEL], "dev");
        assert_eq!(labels[CONTEXT_LABEL], "ferrodock-dev");
        let host = server.host_config.unwrap();
        assert_eq!(host.privileged, Some(true));
        assert_eq!(host.network_mode.as_deref(), Some("ferrodock-kube-dev"));
        let ports = host.port_bindings.unwrap();
        assert_eq!(
            ports["6443/tcp"].as_ref().unwrap()[0].host_port.as_deref(),
            Some("6550")
        );
        assert_eq!(
            ports["80/tcp"].as_ref().unwrap()[0].host_port.as_deref(),
            Some("8080")
        );

        let worker = spec.worker(2, "t0k3n");
        assert_eq!(worker.hostname.as_deref(), Some("dev-worker-2"));
        assert_eq!(worker.cmd.unwrap()[0], "agent");
        assert!(
            worker
                .env
                .unwrap()
                .contains(&"K3S_URL=https://dev-server:6443".to_owned())
        );
        assert_eq!(worker.labels.unwrap()[ROLE_LABEL], "worker");
        assert_eq!(spec.version(), "1.37");
        assert_ne!(token(), token());
    }

    #[test]
    fn reads_nodes_and_pods() {
        let nodes = r#"{"items":[
            {"metadata":{"name":"dev-worker-1","labels":{}},
             "status":{"conditions":[{"type":"Ready","status":"False"}],"nodeInfo":{"kubeletVersion":"v1.37.1+k3s1"}}},
            {"metadata":{"name":"dev-server","labels":{"node-role.kubernetes.io/control-plane":"true","node-role.kubernetes.io/master":"true"}},
             "status":{"conditions":[{"type":"Ready","status":"True"}],"nodeInfo":{"kubeletVersion":"v1.37.1+k3s1"}}}]}"#;
        let nodes = parse_nodes(nodes).unwrap();
        assert_eq!(nodes[0].name, "dev-server");
        assert!(nodes[0].ready);
        assert_eq!(nodes[0].roles, "control-plane,master");
        assert!(!nodes[1].ready);

        let pods = r#"{"items":[
            {"metadata":{"namespace":"kube-system","name":"coredns-1"},"spec":{"nodeName":"dev-server","containers":[{}]},
             "status":{"phase":"Running","containerStatuses":[{"ready":true,"restartCount":0,"state":{"running":{}}}]}},
            {"metadata":{"namespace":"default","name":"web-1"},"spec":{"nodeName":"dev-worker-1","containers":[{},{}]},
             "status":{"phase":"Pending","containerStatuses":[{"ready":false,"restartCount":2,"state":{"waiting":{"reason":"ImagePullBackOff"}}},{"ready":true,"restartCount":1,"state":{"running":{}}}]}},
            {"metadata":{"namespace":"kube-system","name":"helm-install-traefik"},"spec":{"containers":[{}]},
             "status":{"phase":"Succeeded","containerStatuses":[{"ready":false,"restartCount":0,"state":{"terminated":{"reason":"Completed"}}}]}}]}"#;
        let pods = parse_pods(pods).unwrap();
        assert_eq!(pods[0].name, "web-1");
        assert_eq!(pods[0].ready, "1/2");
        assert_eq!(pods[0].status, "ImagePullBackOff");
        assert_eq!(pods[0].restarts, 3);
        assert!(!pods[0].healthy());
        assert!(pods[1].healthy());
        assert_eq!(pods[2].status, "Completed");
        assert!(pods[2].healthy());
    }
}

#[cfg(test)]
mod live {
    use super::*;

    /// Talks to a real daemon: `cargo test -- --ignored`. Needs
    /// `rancher/k3s:<newest tag>` and, to run a pod without a registry,
    /// `rancher/mirrored-pause:3.10.2` and `busybox:latest`. Makes a cluster
    /// with a worker, loads both images into it, runs a pod on them, then
    /// removes it all.
    #[tokio::test]
    #[ignore]
    async fn live_daemon_runs_a_cluster() {
        let client = DockerClient::connect().unwrap();
        let spec = ClusterSpec {
            name: "ferrodock-test".into(),
            tag: KUBE_VERSIONS[0].1.into(),
            workers: 1,
            api_port: 16550,
            ingress_port: 18080,
            kubectl: false,
        };
        let _ = client.remove_cluster(&spec.name).await;
        let mut steps = Vec::new();
        let kubeconfig = client
            .create_cluster(&spec, |s| steps.push(s.to_owned()))
            .await
            .unwrap();
        assert!(
            kubeconfig.contains("certificate-authority-data"),
            "{kubeconfig}"
        );
        assert_eq!(steps.len(), 4, "{steps:?}");

        for image in ["rancher/mirrored-pause:3.10.2", "busybox:latest"] {
            client.load_into_cluster(&spec.name, image).await.unwrap();
        }
        let server = server_name(&spec.name);
        let run = [
            "kubectl",
            "run",
            "hello",
            "--image=busybox:latest",
            "--image-pull-policy=Never",
            "--restart=Never",
            "--",
            "sh",
            "-c",
            "echo hello && sleep 600",
        ];
        let (code, _, err) = client.kube_exec(&server, &run).await.unwrap();
        assert_eq!(code, Some(0), "{err}");
        let mut status = ClusterStatus::default();
        for _ in 0..60 {
            status = client.cluster_status(&spec.name).await.unwrap();
            let both = status.nodes.len() == 2 && status.nodes.iter().all(|n| n.ready);
            if both && status.pods.iter().any(|p| p.name == "hello" && p.healthy()) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
        assert!(status.nodes.iter().all(|n| n.ready), "{status:?}");
        let hello = status.pods.iter().find(|p| p.name == "hello").unwrap();
        assert_eq!(hello.status, "Running", "{status:?}");

        client.stop_cluster(&spec.name).await.unwrap();
        client.start_cluster(&spec.name).await.unwrap();
        client.remove_cluster(&spec.name).await.unwrap();
        assert!(client.cluster_nodes(&spec.name).await.unwrap().is_empty());
    }
}
