//! `docker inspect` for any resource: the raw JSON, plus the parts people
//! look for most (environment, labels, restart policy, health).

use bollard::query_parameters::{InspectContainerOptions, InspectNetworkOptions};
use serde_json::Value;

use super::{DockerClient, Resource, describe};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inspection {
    /// The ID (a volume's name) this belongs to.
    pub id: String,
    /// The full inspect output, pretty-printed.
    pub json: String,
    /// `KEY=value` environment variables, split.
    pub env: Vec<(String, String)>,
    pub labels: Vec<(String, String)>,
    pub restart_policy: Option<String>,
    pub health: Option<Health>,
    /// Container ports the image declares with `EXPOSE`, e.g. `80/tcp`.
    pub exposed_ports: Vec<String>,
    /// For containers, the settings needed to make a copy of it.
    pub run: RunSettings,
}

/// How a container was started, as far as the Run dialog can recreate it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RunSettings {
    pub image: String,
    /// (host port, container port), TCP.
    pub ports: Vec<(u16, u16)>,
    /// (source, target) bind mounts and named volumes.
    pub volumes: Vec<(String, String)>,
    /// The network, unless it's the default one.
    pub network: String,
    /// The command, when it was set explicitly.
    pub command: String,
}

fn run_settings(value: &Value) -> RunSettings {
    let host = &value["HostConfig"];
    let mut ports: Vec<(u16, u16)> = host["PortBindings"]
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(port, bindings)| {
            let container = port.strip_suffix("/tcp")?.parse().ok()?;
            let host = bindings[0]["HostPort"].as_str()?.parse().ok()?;
            Some((host, container))
        })
        .collect();
    ports.sort();
    let volumes = host["Binds"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter_map(|bind| {
            let mut parts = bind.splitn(3, ':');
            Some((parts.next()?.to_owned(), parts.next()?.to_owned()))
        })
        .collect();
    let network = host["NetworkMode"]
        .as_str()
        .filter(|n| !matches!(*n, "default" | "bridge" | ""))
        .unwrap_or_default()
        .to_owned();
    let command = value["Config"]["Cmd"]
        .as_array()
        .map(|cmd| {
            cmd.iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default();
    RunSettings {
        image: value["Config"]["Image"]
            .as_str()
            .unwrap_or_default()
            .to_owned(),
        ports,
        volumes,
        network,
        command,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Health {
    /// `starting`, `healthy` or `unhealthy`.
    pub status: String,
    pub failing_streak: i64,
    /// Output of the most recent check.
    pub last_output: String,
}

impl Inspection {
    fn from_json(id: &str, value: &Value) -> Self {
        let config = &value["Config"];
        let env = config["Env"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(|line| match line.split_once('=') {
                Some((key, val)) => (key.to_owned(), val.to_owned()),
                None => (line.to_owned(), String::new()),
            })
            .collect();
        // Containers and images keep labels in Config; volumes and networks
        // at the top level.
        let labels_obj = if config["Labels"].is_object() {
            &config["Labels"]
        } else {
            &value["Labels"]
        };
        let mut labels: Vec<(String, String)> = labels_obj
            .as_object()
            .into_iter()
            .flatten()
            .map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_owned()))
            .collect();
        labels.sort();
        let restart_policy = value["HostConfig"]["RestartPolicy"]["Name"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
        let health = value["State"]["Health"].as_object().map(|h| Health {
            status: h["Status"].as_str().unwrap_or_default().to_owned(),
            failing_streak: h["FailingStreak"].as_i64().unwrap_or_default(),
            last_output: h["Log"]
                .as_array()
                .and_then(|log| log.last())
                .and_then(|entry| entry["Output"].as_str())
                .unwrap_or_default()
                .trim()
                .to_owned(),
        });
        let mut exposed_ports: Vec<String> = config["ExposedPorts"]
            .as_object()
            .into_iter()
            .flatten()
            .map(|(port, _)| port.clone())
            .collect();
        exposed_ports.sort();
        Self {
            id: id.to_owned(),
            exposed_ports,
            run: run_settings(value),
            json: serde_json::to_string_pretty(value).unwrap_or_default(),
            env,
            labels,
            restart_policy,
            health,
        }
    }
}

/// Whether an environment variable probably holds a secret, so its value
/// is hidden until the user asks to see it.
pub fn looks_secret(key: &str) -> bool {
    const HINTS: [&str; 8] = [
        "PASSWORD",
        "PASSWD",
        "SECRET",
        "TOKEN",
        "KEY",
        "CREDENTIAL",
        "AUTH",
        "PRIVATE",
    ];
    let key = key.to_uppercase();
    HINTS.iter().any(|hint| key.contains(hint))
}

impl DockerClient {
    pub async fn inspect(&self, resource: Resource, id: &str) -> Result<Inspection, String> {
        let docker = &self.docker;
        let value = match resource {
            Resource::Containers => docker
                .inspect_container(id, None::<InspectContainerOptions>)
                .await
                .map(serde_json::to_value),
            Resource::Images => docker.inspect_image(id).await.map(serde_json::to_value),
            Resource::Volumes => docker.inspect_volume(id).await.map(serde_json::to_value),
            Resource::Networks => docker
                .inspect_network(id, None::<InspectNetworkOptions>)
                .await
                .map(serde_json::to_value),
            Resource::Disk => return Err("disk usage can't be inspected".into()),
        }
        .map_err(describe)?
        .map_err(|e| e.to_string())?;
        Ok(Inspection::from_json(id, &value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_out_the_useful_parts() {
        let value = serde_json::json!({
            "Config": {
                "Env": ["PATH=/usr/bin", "DB_PASSWORD=hunter2", "EMPTY"],
                "Labels": {"b": "2", "a": "1"}
            },
            "HostConfig": {"RestartPolicy": {"Name": "unless-stopped"}},
            "State": {"Health": {
                "Status": "unhealthy",
                "FailingStreak": 3,
                "Log": [{"Output": "old"}, {"Output": "curl: (7) refused\n"}]
            }}
        });
        let i = Inspection::from_json("abc", &value);
        assert_eq!(i.env[1], ("DB_PASSWORD".into(), "hunter2".into()));
        assert_eq!(i.env[2], ("EMPTY".into(), String::new()));
        assert_eq!(i.labels[0], ("a".into(), "1".into()));
        assert_eq!(i.restart_policy.as_deref(), Some("unless-stopped"));
        let health = i.health.unwrap();
        assert_eq!(health.failing_streak, 3);
        assert_eq!(health.last_output, "curl: (7) refused");
        assert!(i.json.contains("\"Env\""));
    }

    #[test]
    fn reads_how_a_container_was_run() {
        let value = serde_json::json!({
            "Config": {"Image": "nginx:alpine", "Cmd": ["nginx", "-g", "daemon off;"]},
            "HostConfig": {
                "PortBindings": {"80/tcp": [{"HostIp": "", "HostPort": "8080"}], "53/udp": []},
                "Binds": ["site:/usr/share/nginx/html:ro"],
                "NetworkMode": "backend"
            }
        });
        let run = Inspection::from_json("x", &value).run;
        assert_eq!(run.image, "nginx:alpine");
        assert_eq!(run.ports, vec![(8080, 80)]);
        assert_eq!(
            run.volumes,
            vec![("site".into(), "/usr/share/nginx/html".into())]
        );
        assert_eq!(run.network, "backend");
        assert_eq!(run.command, "nginx -g daemon off;");
    }

    #[test]
    fn volume_labels_are_top_level() {
        let value = serde_json::json!({"Name": "data", "Labels": {"x": "y"}});
        let i = Inspection::from_json("data", &value);
        assert_eq!(i.labels, vec![("x".into(), "y".into())]);
        assert!(i.health.is_none());
    }

    #[test]
    fn spots_secrets() {
        assert!(looks_secret("DB_PASSWORD"));
        assert!(looks_secret("github_token"));
        assert!(looks_secret("AWS_SECRET_ACCESS_KEY"));
        assert!(!looks_secret("PATH"));
    }

    /// Talks to a real daemon: `cargo test -- --ignored`.
    #[tokio::test]
    #[ignore]
    async fn live_daemon_inspects_a_container() {
        let client = DockerClient::connect().unwrap();
        let Some(c) = client.list_containers().await.unwrap().into_iter().next() else {
            return;
        };
        let i = client.inspect(Resource::Containers, &c.id).await.unwrap();
        assert!(i.json.contains(&c.id));
        assert!(!i.env.is_empty());
    }
}
