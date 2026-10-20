//! Quick start templates: popular services with settings that work for
//! local development, ready to run in one step.

use super::{ContainerInfo, Restart, RunSpec};

/// A setting passed to the container as an environment variable.
pub struct Field {
    pub label: &'static str,
    pub env: &'static str,
    pub default: &'static str,
}

/// A port the service listens on, and the host port it gets by default.
pub struct Port {
    pub container: u16,
    pub host: u16,
    /// What it's for, when there's more than one ("Web UI").
    pub label: &'static str,
}

pub struct Template {
    pub name: &'static str,
    pub summary: &'static str,
    /// The image without a tag.
    pub image: &'static str,
    /// Tags offered, newest first; the first is the default.
    pub tags: &'static [&'static str],
    pub ports: &'static [Port],
    pub fields: &'static [Field],
    /// Where the service keeps its data, to put in a volume.
    pub data: Option<&'static str>,
    /// Overrides the image's command when not empty.
    pub command: &'static str,
    /// How to connect: `{port}` is the first host port, `{ENV}` a field.
    pub connection: &'static str,
    /// Brand-ish colour for its badge.
    pub color: [u8; 3],
}

const fn port(container: u16, host: u16, label: &'static str) -> Port {
    Port {
        container,
        host,
        label,
    }
}

const fn field(label: &'static str, env: &'static str, default: &'static str) -> Field {
    Field {
        label,
        env,
        default,
    }
}

/// Every template, in the order they're shown.
pub const TEMPLATES: &[Template] = &[
    Template {
        name: "Postgres",
        summary: "SQL database",
        image: "postgres",
        tags: &["17", "16", "15"],
        ports: &[port(5432, 5432, "")],
        fields: &[
            field("User", "POSTGRES_USER", "postgres"),
            field("Password", "POSTGRES_PASSWORD", "postgres"),
            field("Database", "POSTGRES_DB", "app"),
        ],
        data: Some("/var/lib/postgresql/data"),
        command: "",
        connection: "postgres://{POSTGRES_USER}:{POSTGRES_PASSWORD}@localhost:{port}/{POSTGRES_DB}",
        color: [51, 103, 145],
    },
    Template {
        name: "MySQL",
        summary: "SQL database",
        image: "mysql",
        tags: &["8.4", "9"],
        ports: &[port(3306, 3306, "")],
        fields: &[
            field("Root password", "MYSQL_ROOT_PASSWORD", "mysql"),
            field("Database", "MYSQL_DATABASE", "app"),
        ],
        data: Some("/var/lib/mysql"),
        command: "",
        connection: "mysql://root:{MYSQL_ROOT_PASSWORD}@localhost:{port}/{MYSQL_DATABASE}",
        color: [0, 117, 143],
    },
    Template {
        name: "Redis",
        summary: "Cache and queues",
        image: "redis",
        tags: &["8", "7"],
        ports: &[port(6379, 6379, "")],
        fields: &[],
        data: Some("/data"),
        command: "",
        connection: "redis://localhost:{port}",
        color: [216, 44, 32],
    },
    Template {
        name: "MongoDB",
        summary: "Document database",
        image: "mongo",
        tags: &["8", "7"],
        ports: &[port(27017, 27017, "")],
        fields: &[
            field("User", "MONGO_INITDB_ROOT_USERNAME", "mongo"),
            field("Password", "MONGO_INITDB_ROOT_PASSWORD", "mongo"),
        ],
        data: Some("/data/db"),
        command: "",
        connection: "mongodb://{MONGO_INITDB_ROOT_USERNAME}:{MONGO_INITDB_ROOT_PASSWORD}@localhost:{port}",
        color: [19, 170, 82],
    },
    Template {
        name: "RabbitMQ",
        summary: "Message broker",
        image: "rabbitmq",
        tags: &["4-management"],
        ports: &[port(5672, 5672, "AMQP"), port(15672, 15672, "Web UI")],
        fields: &[
            field("User", "RABBITMQ_DEFAULT_USER", "guest"),
            field("Password", "RABBITMQ_DEFAULT_PASS", "guest"),
        ],
        data: Some("/var/lib/rabbitmq"),
        command: "",
        connection: "amqp://{RABBITMQ_DEFAULT_USER}:{RABBITMQ_DEFAULT_PASS}@localhost:{port}",
        color: [255, 102, 0],
    },
    Template {
        name: "MinIO",
        summary: "S3-compatible storage",
        image: "minio/minio",
        tags: &["latest"],
        ports: &[port(9000, 9000, "S3 API"), port(9001, 9001, "Console")],
        fields: &[
            field("User", "MINIO_ROOT_USER", "minioadmin"),
            field("Password", "MINIO_ROOT_PASSWORD", "minioadmin"),
        ],
        data: Some("/data"),
        command: "server /data --console-address :9001",
        connection: "http://localhost:{port}",
        color: [199, 46, 73],
    },
    Template {
        name: "Mailpit",
        summary: "Catches outgoing email",
        image: "axllent/mailpit",
        tags: &["latest"],
        ports: &[port(1025, 1025, "SMTP"), port(8025, 8025, "Web UI")],
        fields: &[],
        data: None,
        command: "",
        connection: "smtp://localhost:{port}",
        color: [47, 111, 222],
    },
    Template {
        name: "Nginx",
        summary: "Web server",
        image: "nginx",
        tags: &["alpine", "latest"],
        ports: &[port(80, 8080, "")],
        fields: &[],
        data: None,
        command: "",
        connection: "http://localhost:{port}",
        color: [38, 149, 57],
    },
];

/// What the user chose for one run of a template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choices {
    pub name: String,
    pub tag: String,
    /// Host port for each of the template's ports.
    pub ports: Vec<u16>,
    /// A value for each of the template's fields.
    pub values: Vec<String>,
    /// Keep the data in a named volume.
    pub keep_data: bool,
}

impl Template {
    /// Short name for container and volume names: "postgres".
    pub fn slug(&self) -> String {
        self.name.to_lowercase()
    }

    /// Defaults that don't clash with what's there: the first free name
    /// and host ports. `port_free` says whether a host port can be used.
    pub fn defaults(&self, names: &[&str], port_free: impl Fn(u16) -> bool) -> Choices {
        let slug = self.slug();
        let name = (1..)
            .map(|n| {
                if n == 1 {
                    slug.clone()
                } else {
                    format!("{slug}-{n}")
                }
            })
            .find(|candidate| !names.contains(&candidate.as_str()))
            .unwrap_or(slug);
        let mut ports: Vec<u16> = Vec::new();
        for p in self.ports {
            let free = (p.host..u16::MAX)
                .find(|candidate| !ports.contains(candidate) && port_free(*candidate))
                .unwrap_or(p.host);
            ports.push(free);
        }
        Choices {
            name,
            tag: self.tags[0].to_owned(),
            ports,
            values: self.fields.iter().map(|f| f.default.to_owned()).collect(),
            keep_data: self.data.is_some(),
        }
    }

    /// Defaults that don't clash with `containers`: a free name, and host
    /// ports no container publishes and nothing on this machine listens
    /// on.
    pub fn defaults_for(&self, containers: &[ContainerInfo]) -> Choices {
        let names: Vec<&str> = containers.iter().map(|c| c.name.as_str()).collect();
        self.defaults(&names, |port| port_free(containers, port))
    }

    /// The image with the chosen tag.
    pub fn reference(&self, choices: &Choices) -> String {
        format!("{}:{}", self.image, choices.tag)
    }

    pub fn spec(&self, choices: &Choices) -> RunSpec {
        let volumes = match (self.data, choices.keep_data) {
            (Some(path), true) => vec![(format!("{}-data", choices.name), path.to_owned())],
            _ => Vec::new(),
        };
        RunSpec {
            image: self.reference(choices),
            name: choices.name.clone(),
            ports: self
                .ports
                .iter()
                .zip(&choices.ports)
                .map(|(p, host)| (*host, p.container))
                .collect(),
            env: self
                .fields
                .iter()
                .zip(&choices.values)
                .map(|(f, v)| (f.env.to_owned(), v.clone()))
                .collect(),
            volumes,
            network: String::new(),
            restart: Restart::UnlessStopped,
            command: self.command.to_owned(),
            nano_cpus: 0,
            memory: 0,
        }
    }

    /// How to connect, filled in with the choices.
    pub fn connection(&self, choices: &Choices) -> String {
        let mut text = self.connection.replace(
            "{port}",
            &choices.ports.first().copied().unwrap_or(0).to_string(),
        );
        for (f, value) in self.fields.iter().zip(&choices.values) {
            text = text.replace(&format!("{{{}}}", f.env), value);
        }
        text
    }

    /// Whether `image` (as a container shows it, e.g. `postgres:17`) is
    /// this template's.
    pub fn is_image(&self, image: &str) -> bool {
        let repo = image.rsplit_once(':').map_or(image, |(repo, _)| repo);
        let repo = repo.strip_prefix("docker.io/").unwrap_or(repo);
        let repo = repo.strip_prefix("library/").unwrap_or(repo);
        repo == self.image
    }
}

/// Changes to a template's defaults, as the CLI and MCP server take
/// them. Anything left empty keeps the default.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Overrides {
    pub name: Option<String>,
    /// The image tag ("version").
    pub tag: Option<String>,
    /// Host ports, in the order of the template's ports.
    pub ports: Vec<u16>,
    /// Field values by their variable, e.g. `POSTGRES_PASSWORD`.
    pub values: Vec<(String, String)>,
    pub keep_data: Option<bool>,
}

impl Template {
    /// `choices` with `overrides` applied. Fails on a value for a field
    /// the template doesn't have, or more ports than it has.
    pub fn customize(
        &self,
        mut choices: Choices,
        overrides: &Overrides,
    ) -> Result<Choices, String> {
        if let Some(name) = overrides.name.as_ref().filter(|n| !n.trim().is_empty()) {
            choices.name = name.trim().to_owned();
        }
        if let Some(tag) = overrides.tag.as_ref().filter(|t| !t.trim().is_empty()) {
            choices.tag = tag.trim().to_owned();
        }
        if overrides.ports.len() > self.ports.len() {
            return Err(format!(
                "{} has {} port(s), not {}",
                self.name,
                self.ports.len(),
                overrides.ports.len()
            ));
        }
        for (slot, port) in choices.ports.iter_mut().zip(&overrides.ports) {
            *slot = *port;
        }
        for (key, value) in &overrides.values {
            let index = self
                .fields
                .iter()
                .position(|f| f.env.eq_ignore_ascii_case(key))
                .ok_or_else(|| {
                    let keys: Vec<&str> = self.fields.iter().map(|f| f.env).collect();
                    if keys.is_empty() {
                        format!("{} has no settings", self.name)
                    } else {
                        format!(
                            "{} has no setting {key}; it has {}",
                            self.name,
                            keys.join(", ")
                        )
                    }
                })?;
            choices.values[index] = value.clone();
        }
        if let Some(keep) = overrides.keep_data {
            choices.keep_data = keep && self.data.is_some();
        }
        Ok(choices)
    }
}

/// Whether a host port is free: none of `containers` publishes it and
/// nothing on this machine listens on it.
pub fn port_free(containers: &[ContainerInfo], port: u16) -> bool {
    let published = containers
        .iter()
        .flat_map(|c| &c.ports)
        .any(|p| p.public == Some(port));
    !published && std::net::TcpListener::bind(("127.0.0.1", port)).is_ok()
}

/// The template called `name` (any case).
pub fn template(name: &str) -> Option<&'static Template> {
    TEMPLATES.iter().find(|t| t.name.eq_ignore_ascii_case(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overrides_change_only_what_they_name() {
        let postgres = template("postgres").unwrap();
        let defaults = postgres.defaults(&[], |_| true);
        let choices = postgres
            .customize(
                defaults.clone(),
                &Overrides {
                    name: Some("db".into()),
                    tag: Some("16".into()),
                    ports: vec![6543],
                    values: vec![("postgres_password".into(), "secret".into())],
                    keep_data: Some(false),
                },
            )
            .unwrap();
        assert_eq!(postgres.reference(&choices), "postgres:16");
        assert_eq!(
            postgres.connection(&choices),
            "postgres://postgres:secret@localhost:6543/app"
        );
        assert!(postgres.spec(&choices).volumes.is_empty());
        assert_eq!(
            postgres.customize(defaults.clone(), &Overrides::default()),
            Ok(defaults.clone())
        );
        let wrong = Overrides {
            values: vec![("NOPE".into(), "x".into())],
            ..Default::default()
        };
        assert!(
            postgres
                .customize(defaults, &wrong)
                .unwrap_err()
                .contains("POSTGRES_PASSWORD")
        );
    }

    #[test]
    fn defaults_avoid_taken_names_and_ports() {
        let postgres = template("postgres").unwrap();
        let choices = postgres.defaults(&["postgres"], |p| p != 5432);
        assert_eq!(choices.name, "postgres-2");
        assert_eq!(choices.ports, [5433]);
        assert_eq!(
            postgres.connection(&choices),
            "postgres://postgres:postgres@localhost:5433/app"
        );
        let spec = postgres.spec(&choices);
        assert_eq!(spec.image, "postgres:17");
        assert_eq!(spec.ports, [(5433, 5432)]);
        assert_eq!(
            spec.volumes,
            [(
                "postgres-2-data".to_owned(),
                "/var/lib/postgresql/data".to_owned()
            )]
        );
        assert!(spec.env.contains(&("POSTGRES_DB".into(), "app".into())));
    }

    #[test]
    fn two_ports_never_share_a_host_port() {
        let rabbit = template("RabbitMQ").unwrap();
        let choices = rabbit.defaults(&[], |_| true);
        assert_eq!(choices.ports, [5672, 15672]);
        let nginx = template("nginx").unwrap();
        assert_eq!(
            nginx.spec(&nginx.defaults(&[], |_| true)).ports,
            [(8080, 80)]
        );
    }

    #[test]
    fn recognises_its_images() {
        let postgres = template("postgres").unwrap();
        assert!(postgres.is_image("postgres:17"));
        assert!(postgres.is_image("docker.io/library/postgres"));
        assert!(!postgres.is_image("postgis/postgis:16"));
        assert!(template("minio").unwrap().is_image("minio/minio:latest"));
    }
}
