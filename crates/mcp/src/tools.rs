//! The MCP tools: the same operations as the `fdock` CLI, answering in
//! the same compact JSON (see `ferrodock_core::summary`). Every call
//! connects afresh, so an engine started after the server just works.
//!
//! Tools that change something are marked so clients ask first; removing
//! a container is marked destructive. `--read-only` leaves them out.

use std::collections::BTreeMap;

use ferrodock_core::connect::{Connection, connect};
use ferrodock_core::summary::{
    Container, Details, Disk, Engine, Image, Network, Project, Started, Template, Volume,
};
use ferrodock_core::{Action, Overrides, Resource, TEMPLATES, engines, group_projects, template};
use futures_util::StreamExt;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{Implementation, ServerCapabilities, ServerConfig};
use rmcp::{ServerHandler, schemars, tool, tool_handler, tool_router};
use serde::{Deserialize, Serialize};

/// The tools that change something, left out with `--read-only`.
pub const CHANGING: [&str; 4] = [
    "container_action",
    "remove_container",
    "pull_image",
    "run_quickstart",
];

/// The most log lines one call returns.
const MAX_LINES: usize = 1000;

#[derive(Clone)]
pub struct Ferrodock {
    engine: Option<String>,
    tool_router: ToolRouter<Self>,
}

impl Ferrodock {
    pub fn new(engine: Option<String>, read_only: bool) -> Self {
        let mut tool_router = Self::tool_router();
        if read_only {
            for name in CHANGING {
                tool_router.remove_route(name);
            }
        }
        Self {
            engine,
            tool_router,
        }
    }

    async fn conn(&self) -> Result<Connection, String> {
        connect(self.engine.as_deref()).await.map_err(|e| {
            format!("{e} (Is the engine running? The engine_status tool lists the engines.)")
        })
    }
}

fn json(value: &impl Serialize) -> Result<String, String> {
    serde_json::to_string(value).map_err(|e| e.to_string())
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListContainers {
    /// Also list stopped containers.
    #[serde(default)]
    pub all: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Logs {
    /// The container's name or ID.
    pub container: String,
    /// How many of the last lines to return (default 100, at most 1000).
    #[serde(default)]
    pub tail: Option<usize>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct InspectParams {
    /// The container's name or ID.
    pub container: String,
    /// Show secret-looking environment values (passwords, tokens) too.
    #[serde(default)]
    pub show_secrets: bool,
}

#[derive(Debug, Clone, Copy, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Verb {
    Start,
    Stop,
    Restart,
    Pause,
    Unpause,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ActionParams {
    /// The container's name or ID.
    pub container: String,
    pub action: Verb,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RemoveParams {
    /// The container's name or ID.
    pub container: String,
    /// Stop it first if it runs.
    #[serde(default)]
    pub force: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PullParams {
    /// The image, e.g. `nginx:alpine`.
    pub image: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct QuickstartParams {
    /// The template, e.g. `postgres` (see list_quickstart_templates).
    pub template: String,
    /// Container name; by default the template's, made unique.
    #[serde(default)]
    pub name: Option<String>,
    /// Image tag, e.g. `16`.
    #[serde(default)]
    pub version: Option<String>,
    /// Host ports, in the order of the template's ports; by default free
    /// ones near the usual ports.
    #[serde(default)]
    pub ports: Vec<u16>,
    /// Settings by variable, e.g. {"POSTGRES_PASSWORD": "secret"}.
    #[serde(default)]
    pub settings: BTreeMap<String, String>,
    /// Keep the data in a volume (default true, where the service has data).
    #[serde(default)]
    pub keep_data: Option<bool>,
}

#[derive(Serialize)]
struct Status {
    engines: Vec<Engine>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[tool_router]
impl Ferrodock {
    /// The Docker engines Ferrodock knows of, which one the tools use and
    /// its Docker version, or why none answered.
    #[tool(annotations(title = "Engine status", read_only_hint = true))]
    async fn engine_status(&self) -> Result<String, String> {
        let picked = self.conn().await;
        let engines = engines()
            .iter()
            .map(|e| {
                let using = picked.as_ref().ok().filter(|c| c.engine.name == e.name);
                Engine {
                    name: e.name.clone(),
                    host: e.host.clone(),
                    in_use: using.is_some(),
                    version: using.map(|c| c.version.clone()),
                }
            })
            .collect();
        json(&Status {
            engines,
            error: picked.err(),
        })
    }

    /// Containers with their state, image, published ports and Compose
    /// project. Running ones only, unless `all`.
    #[tool(annotations(title = "List containers", read_only_hint = true))]
    async fn list_containers(
        &self,
        Parameters(p): Parameters<ListContainers>,
    ) -> Result<String, String> {
        let containers = self.conn().await?.client.list_containers().await?;
        let shown: Vec<Container> = containers
            .iter()
            .filter(|c| p.all || c.is_active())
            .map(Container::from)
            .collect();
        json(&shown)
    }

    /// The last lines of a container's logs (stdout and stderr).
    #[tool(annotations(title = "Container logs", read_only_hint = true))]
    async fn container_logs(&self, Parameters(p): Parameters<Logs>) -> Result<String, String> {
        let tail = p.tail.unwrap_or(100).clamp(1, MAX_LINES);
        let conn = self.conn().await?;
        let mut stream = Box::pin(conn.client.logs_with(&p.container, tail, false, false));
        let mut text = String::new();
        while let Some(chunk) = stream.next().await {
            text.push_str(&chunk?);
        }
        let lines: Vec<&str> = text.lines().collect();
        json(&serde_json::json!({ "lines": lines }))
    }

    /// What matters about a container: image, environment (secrets
    /// hidden), ports, mounts, network, command, restart policy, health
    /// and labels.
    #[tool(annotations(title = "Inspect a container", read_only_hint = true))]
    async fn inspect_container(
        &self,
        Parameters(p): Parameters<InspectParams>,
    ) -> Result<String, String> {
        let inspection = self
            .conn()
            .await?
            .client
            .inspect(Resource::Containers, &p.container)
            .await?;
        json(&Details::new(&inspection, p.show_secrets))
    }

    /// Images with their size and age.
    #[tool(annotations(title = "List images", read_only_hint = true))]
    async fn list_images(&self) -> Result<String, String> {
        let images = self.conn().await?.client.list_images().await?;
        json(&images.iter().map(Image::from).collect::<Vec<_>>())
    }

    /// Volumes.
    #[tool(annotations(title = "List volumes", read_only_hint = true))]
    async fn list_volumes(&self) -> Result<String, String> {
        let volumes = self.conn().await?.client.list_volumes().await?;
        json(&volumes.iter().map(Volume::from).collect::<Vec<_>>())
    }

    /// Networks with their driver and subnets.
    #[tool(annotations(title = "List networks", read_only_hint = true))]
    async fn list_networks(&self) -> Result<String, String> {
        let networks = self.conn().await?.client.list_networks().await?;
        json(&networks.iter().map(Network::from).collect::<Vec<_>>())
    }

    /// Docker Compose projects: their services, how many containers run,
    /// and their folder.
    #[tool(annotations(title = "List Compose projects", read_only_hint = true))]
    async fn list_projects(&self) -> Result<String, String> {
        let containers = self.conn().await?.client.list_containers().await?;
        let projects: Vec<Project> = group_projects(&containers)
            .iter()
            .map(Project::from)
            .collect();
        json(&projects)
    }

    /// Disk space used by images, containers, volumes and the build cache,
    /// and how much could be freed.
    #[tool(annotations(title = "Disk usage", read_only_hint = true))]
    async fn disk_usage(&self) -> Result<String, String> {
        let usage = self.conn().await?.client.disk_usage().await?;
        if !usage.is_reported() {
            return Err(
                "This engine doesn't report disk usage (it needs Docker Engine 29 or newer)".into(),
            );
        }
        json(&Disk::from(&usage))
    }

    /// Popular services (Postgres, MySQL, Redis, MongoDB…) that
    /// run_quickstart starts with settings for local development: their
    /// image, ports and the settings that can be changed.
    #[tool(annotations(title = "Quick start templates", read_only_hint = true))]
    async fn list_quickstart_templates(&self) -> Result<String, String> {
        json(&TEMPLATES.iter().map(Template::from).collect::<Vec<_>>())
    }

    /// Starts, stops, restarts, pauses or resumes a container.
    #[tool(annotations(
        title = "Start, stop or restart a container",
        read_only_hint = false,
        destructive_hint = false,
        idempotent_hint = true,
        open_world_hint = false
    ))]
    async fn container_action(
        &self,
        Parameters(p): Parameters<ActionParams>,
    ) -> Result<String, String> {
        let action = match p.action {
            Verb::Start => Action::Start,
            Verb::Stop => Action::Stop,
            Verb::Restart => Action::Restart,
            Verb::Pause => Action::Pause,
            Verb::Unpause => Action::Unpause,
        };
        self.conn()
            .await?
            .client
            .perform(action, &p.container)
            .await?;
        Ok(format!("{} {}", action.past_tense(), p.container))
    }

    /// Removes a container (stopped, or any with `force`). Its named
    /// volumes are kept.
    #[tool(annotations(
        title = "Remove a container",
        read_only_hint = false,
        destructive_hint = true,
        idempotent_hint = false,
        open_world_hint = false
    ))]
    async fn remove_container(
        &self,
        Parameters(p): Parameters<RemoveParams>,
    ) -> Result<String, String> {
        let action = if p.force {
            Action::StopAndRemove
        } else {
            Action::Remove(Resource::Containers)
        };
        self.conn()
            .await?
            .client
            .perform(action, &p.container)
            .await?;
        Ok(format!("Removed {}", p.container))
    }

    /// Pulls an image from its registry (Docker Hub unless the name says
    /// otherwise), with the Docker CLI's saved logins.
    #[tool(annotations(
        title = "Pull an image",
        read_only_hint = false,
        destructive_hint = false,
        idempotent_hint = true,
        open_world_hint = true
    ))]
    async fn pull_image(&self, Parameters(p): Parameters<PullParams>) -> Result<String, String> {
        self.conn().await?.client.pull(&p.image, |_| {}).await?;
        Ok(format!("Pulled {}", p.image))
    }

    /// Starts a popular service from a Quick start template, with free
    /// ports and a unique name unless given, pulling its image first if
    /// needed. Returns the container's name, ports and how to connect.
    #[tool(annotations(
        title = "Run a Quick start service",
        read_only_hint = false,
        destructive_hint = false,
        idempotent_hint = false,
        open_world_hint = true
    ))]
    async fn run_quickstart(
        &self,
        Parameters(p): Parameters<QuickstartParams>,
    ) -> Result<String, String> {
        let template = template(&p.template).ok_or_else(|| {
            let names: Vec<String> = TEMPLATES.iter().map(|t| t.slug()).collect();
            format!(
                "No template called {}. Templates: {}",
                p.template,
                names.join(", ")
            )
        })?;
        let conn = self.conn().await?;
        let containers = conn.client.list_containers().await?;
        let overrides = Overrides {
            name: p.name,
            tag: p.version,
            ports: p.ports,
            values: p.settings.into_iter().collect(),
            keep_data: p.keep_data,
        };
        let choices = template.customize(template.defaults_for(&containers), &overrides)?;
        conn.client.run_template(template, &choices, |_| {}).await?;
        json(&Started::new(template, &choices))
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for Ferrodock {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(
                "ferrodock-mcp",
                env!("CARGO_PKG_VERSION"),
            ))
            .with_instructions(
                "Ferrodock's Docker tools: see and manage containers, images, volumes, \
                 networks and Compose projects, read logs, and start popular services \
                 (Postgres, Redis…) for local development with run_quickstart. Answers \
                 are compact JSON. Containers are named by name or ID.",
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_only_leaves_out_what_changes_things() {
        let all = Ferrodock::new(None, false).tool_router.list_all();
        let names: Vec<&str> = all.iter().map(|t| t.name.as_ref()).collect();
        for name in CHANGING {
            assert!(names.contains(&name), "{name} missing from {names:?}");
        }
        assert_eq!(names.len(), 14);
        // Every tool says what it does, and whether it only looks.
        for tool in &all {
            assert!(
                tool.description.as_ref().is_some_and(|d| !d.is_empty()),
                "{}",
                tool.name
            );
            let looks = tool.annotations.as_ref().and_then(|a| a.read_only_hint);
            assert_eq!(
                looks,
                Some(!CHANGING.contains(&tool.name.as_ref())),
                "{}",
                tool.name
            );
        }
        let read_only = Ferrodock::new(None, true).tool_router.list_all();
        assert_eq!(read_only.len(), 10);
        assert!(
            read_only
                .iter()
                .all(|t| !CHANGING.contains(&t.name.as_ref()))
        );
        let remove = all.iter().find(|t| t.name == "remove_container").unwrap();
        assert_eq!(
            remove.annotations.as_ref().and_then(|a| a.destructive_hint),
            Some(true)
        );
    }
}
