//! Running an image as a new container, and pulling images.

use std::collections::HashMap;

use bollard::models::{
    ContainerCreateBody, HostConfig, PortBinding, RestartPolicy, RestartPolicyNameEnum,
};
use bollard::query_parameters::{
    CreateContainerOptionsBuilder, CreateImageOptions, StartContainerOptions,
};
use futures_util::StreamExt;

use super::{DockerClient, describe, limits};

/// Everything the Run dialog asks for: a small subset of `docker run`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RunSpec {
    pub image: String,
    /// Empty lets Docker pick a name.
    pub name: String,
    /// (host port, container port), TCP.
    pub ports: Vec<(u16, u16)>,
    pub env: Vec<(String, String)>,
    /// (host path or volume name, path in the container).
    pub volumes: Vec<(String, String)>,
    /// Empty means the default bridge network.
    pub network: String,
    pub restart: Restart,
    /// Overrides the image's command when not empty.
    pub command: String,
    /// CPU limit in nano-CPUs; 0 for none.
    pub nano_cpus: i64,
    /// Memory limit in bytes; 0 for none.
    pub memory: i64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Restart {
    #[default]
    No,
    Always,
    UnlessStopped,
    OnFailure,
}

impl Restart {
    pub const ALL: [Restart; 4] = [
        Restart::No,
        Restart::Always,
        Restart::UnlessStopped,
        Restart::OnFailure,
    ];

    /// The policy named `name` (`always`, `unless-stopped`, …).
    pub fn from_name(name: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|r| r.as_str() == name)
            .unwrap_or(Restart::No)
    }

    pub(super) fn engine_name(self) -> RestartPolicyNameEnum {
        match self {
            Restart::No => RestartPolicyNameEnum::NO,
            Restart::Always => RestartPolicyNameEnum::ALWAYS,
            Restart::UnlessStopped => RestartPolicyNameEnum::UNLESS_STOPPED,
            Restart::OnFailure => RestartPolicyNameEnum::ON_FAILURE,
        }
    }

    /// The name `docker run --restart` uses.
    pub fn as_str(self) -> &'static str {
        match self {
            Restart::No => "no",
            Restart::Always => "always",
            Restart::UnlessStopped => "unless-stopped",
            Restart::OnFailure => "on-failure",
        }
    }
}

/// Quotes a shell word when it needs it, for the command preview.
fn shell_word(word: &str) -> String {
    let plain = !word.is_empty()
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./:=@%+,".contains(c));
    if plain {
        word.to_owned()
    } else {
        format!("'{}'", word.replace('\'', r"'\''"))
    }
}

impl RunSpec {
    /// The equivalent `docker run` command, for copying.
    pub fn docker_command(&self) -> String {
        let mut parts = vec!["docker run -d".to_owned()];
        if !self.name.trim().is_empty() {
            parts.push(format!("--name {}", shell_word(self.name.trim())));
        }
        for (host, container) in &self.ports {
            parts.push(format!("-p {host}:{container}"));
        }
        for (key, value) in self.env.iter().filter(|(k, _)| !k.trim().is_empty()) {
            parts.push(format!(
                "-e {}",
                shell_word(&format!("{}={value}", key.trim()))
            ));
        }
        for (source, target) in self
            .volumes
            .iter()
            .filter(|(s, t)| !s.is_empty() && !t.is_empty())
        {
            parts.push(format!("-v {}", shell_word(&format!("{source}:{target}"))));
        }
        if !self.network.is_empty() {
            parts.push(format!("--network {}", shell_word(&self.network)));
        }
        if self.restart != Restart::No {
            parts.push(format!("--restart {}", self.restart.as_str()));
        }
        if self.nano_cpus > 0 {
            parts.push(format!("--cpus {}", limits::cpus_text(self.nano_cpus)));
        }
        if self.memory > 0 {
            parts.push(format!("--memory {}", limits::memory_text(self.memory)));
        }
        parts.push(shell_word(&self.image));
        if !self.command.trim().is_empty() {
            parts.push(self.command.trim().to_owned());
        }
        parts.join(" ")
    }

    fn body(&self) -> ContainerCreateBody {
        let port_bindings: HashMap<String, Option<Vec<PortBinding>>> = self
            .ports
            .iter()
            .map(|(host, container)| {
                (
                    format!("{container}/tcp"),
                    Some(vec![PortBinding {
                        host_ip: None,
                        host_port: Some(host.to_string()),
                    }]),
                )
            })
            .collect();
        let env: Vec<String> = self
            .env
            .iter()
            .filter(|(k, _)| !k.trim().is_empty())
            .map(|(k, v)| format!("{}={v}", k.trim()))
            .collect();
        let binds: Vec<String> = self
            .volumes
            .iter()
            .filter(|(s, t)| !s.is_empty() && !t.is_empty())
            .map(|(s, t)| format!("{s}:{t}"))
            .collect();
        let restart = self.restart.engine_name();
        ContainerCreateBody {
            image: Some(self.image.clone()),
            env: (!env.is_empty()).then_some(env),
            exposed_ports: (!self.ports.is_empty())
                .then(|| self.ports.iter().map(|(_, c)| format!("{c}/tcp")).collect()),
            cmd: (!self.command.trim().is_empty())
                .then(|| self.command.split_whitespace().map(str::to_owned).collect()),
            host_config: Some(HostConfig {
                port_bindings: (!port_bindings.is_empty()).then_some(port_bindings),
                binds: (!binds.is_empty()).then_some(binds),
                network_mode: (!self.network.is_empty()).then(|| self.network.clone()),
                restart_policy: Some(RestartPolicy {
                    name: Some(restart),
                    maximum_retry_count: None,
                }),
                nano_cpus: (self.nano_cpus > 0).then_some(self.nano_cpus),
                memory: (self.memory > 0).then_some(self.memory),
                ..Default::default()
            }),
            ..Default::default()
        }
    }
}

/// How far a pull has got, summed over its layers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PullProgress {
    pub done: u64,
    /// Grows as Docker learns the size of more layers.
    pub total: u64,
    /// Docker's latest status line, e.g. "Downloading".
    pub status: String,
    /// A push rather than a pull.
    pub pushing: bool,
}

impl DockerClient {
    /// Creates and starts a container. Returns its ID.
    pub async fn run(&self, spec: &RunSpec) -> Result<String, String> {
        let mut options = CreateContainerOptionsBuilder::new();
        if !spec.name.trim().is_empty() {
            options = options.name(spec.name.trim());
        }
        let created = self
            .docker
            .create_container(Some(options.build()), spec.body())
            .await
            .map_err(describe)?;
        self.docker
            .start_container(&created.id, None::<StartContainerOptions>)
            .await
            .map_err(describe)?;
        Ok(created.id)
    }

    /// Runs a Quick start template with `choices`, pulling its image first
    /// when it isn't here. Returns the container's ID.
    pub async fn run_template(
        &self,
        template: &super::Template,
        choices: &super::Choices,
        progress: impl FnMut(PullProgress),
    ) -> Result<String, String> {
        let image = template.reference(choices);
        if !self.has_image(&image).await {
            self.pull(&image, progress).await?;
        }
        self.run(&template.spec(choices)).await
    }

    /// Whether `image` (a reference like `nginx:alpine`, or an ID) is here.
    pub async fn has_image(&self, image: &str) -> bool {
        self.docker.inspect_image(image).await.is_ok()
    }

    /// Pulls `image` (e.g. `nginx:alpine`), calling `progress` as layers
    /// download.
    pub async fn pull(
        &self,
        image: &str,
        mut progress: impl FnMut(PullProgress),
    ) -> Result<(), String> {
        let (from_image, tag) = split_tag(image);
        let options = CreateImageOptions {
            from_image: Some(from_image.to_owned()),
            tag: Some(tag.to_owned()),
            ..Default::default()
        };
        let credentials = super::registry::credentials_for(image);
        let mut stream = self.docker.create_image(Some(options), None, credentials);
        let mut layers: HashMap<String, (u64, u64)> = HashMap::new();
        while let Some(item) = stream.next().await {
            let info = item.map_err(describe)?;
            if let Some(error) = info.error_detail.and_then(|e| e.message) {
                return Err(error);
            }
            if let (Some(id), Some(detail)) = (&info.id, &info.progress_detail) {
                let entry = layers.entry(id.clone()).or_default();
                if let Some(total) = detail.total.filter(|t| *t > 0) {
                    entry.1 = total as u64;
                }
                if let Some(current) = detail.current {
                    entry.0 = current as u64;
                }
            }
            if info.status.as_deref() == Some("Pull complete")
                && let Some(entry) = info.id.as_ref().and_then(|id| layers.get_mut(id))
            {
                entry.0 = entry.1;
            }
            progress(PullProgress {
                done: layers.values().map(|(d, _)| d).sum(),
                total: layers.values().map(|(_, t)| t).sum(),
                status: info.status.unwrap_or_default(),
                pushing: false,
            });
        }
        Ok(())
    }
}

/// `nginx:alpine` → (`nginx`, `alpine`); no tag means `latest`. A port in
/// a registry address (`localhost:5000/app`) isn't a tag.
fn split_tag(image: &str) -> (&str, &str) {
    let image = image.trim();
    match image.rsplit_once(':') {
        Some((name, tag)) if !tag.contains('/') => (name, tag),
        _ => (image, "latest"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn previews_the_docker_command() {
        let spec = RunSpec {
            image: "nginx:alpine".into(),
            name: "web".into(),
            ports: vec![(8080, 80)],
            env: vec![
                ("GREETING".into(), "hello world".into()),
                (" ".into(), "x".into()),
            ],
            volumes: vec![("site".into(), "/usr/share/nginx/html".into())],
            network: String::new(),
            restart: Restart::UnlessStopped,
            command: String::new(),
            nano_cpus: 500_000_000,
            memory: 256 * 1024 * 1024,
        };
        assert_eq!(
            spec.docker_command(),
            "docker run -d --name web -p 8080:80 -e 'GREETING=hello world' \
             -v site:/usr/share/nginx/html --restart unless-stopped --cpus 0.5 \
             --memory 256m nginx:alpine"
        );
    }

    #[test]
    fn builds_the_create_body() {
        let spec = RunSpec {
            image: "redis".into(),
            ports: vec![(6380, 6379)],
            ..Default::default()
        };
        let body = spec.body();
        let host = body.host_config.unwrap();
        let binding = &host.port_bindings.unwrap()["6379/tcp"];
        assert_eq!(
            binding.as_ref().unwrap()[0].host_port.as_deref(),
            Some("6380")
        );
        assert_eq!(body.exposed_ports.unwrap(), vec!["6379/tcp"]);
        assert!(body.env.is_none());
    }

    #[test]
    fn splits_tags() {
        assert_eq!(split_tag("nginx:alpine"), ("nginx", "alpine"));
        assert_eq!(split_tag("nginx"), ("nginx", "latest"));
        assert_eq!(
            split_tag("localhost:5000/app"),
            ("localhost:5000/app", "latest")
        );
        assert_eq!(
            split_tag("localhost:5000/app:1.2"),
            ("localhost:5000/app", "1.2")
        );
    }

    /// Talks to a real daemon: `cargo test -- --ignored`. Runs a container
    /// from the first local image, then removes it.
    #[tokio::test]
    #[ignore]
    async fn live_daemon_runs_an_image() {
        use bollard::query_parameters::RemoveContainerOptions;
        let client = DockerClient::connect().unwrap();
        let Some(image) = client.list_images().await.unwrap().into_iter().next() else {
            return;
        };
        let spec = RunSpec {
            image: image.id.clone(),
            name: "ferrodock-live-test".into(),
            env: vec![("HELLO".into(), "world".into())],
            ..Default::default()
        };
        let id = client.run(&spec).await.unwrap();
        let opts = RemoveContainerOptions {
            force: true,
            ..Default::default()
        };
        client
            .docker
            .remove_container(&id, Some(opts))
            .await
            .unwrap();
    }
}
