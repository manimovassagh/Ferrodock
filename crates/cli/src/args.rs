//! What `fdock` takes on the command line.

use std::path::PathBuf;

use clap::builder::PossibleValuesParser;
use clap::{Parser, Subcommand};
use ferrodock_core::CLEAN_TARGETS;

#[derive(Debug, Parser)]
#[command(
    name = "fdock",
    version,
    about = "Ferrodock on the command line: containers, images, logs and Quick start \
             templates, on Ferrodock's own engine or any other Docker engine."
)]
pub struct Cli {
    /// Print compact JSON instead of tables, for scripts and AI tools.
    #[arg(long, global = true)]
    pub json: bool,

    /// The engine to use (see `fdock engine`). By default: DOCKER_HOST when
    /// set, else Ferrodock's own engine when it runs, else the current
    /// Docker context.
    #[arg(long, global = true, value_name = "NAME")]
    pub engine: Option<String>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// List containers (running ones; -a for all).
    Ps {
        /// Also stopped containers.
        #[arg(short, long)]
        all: bool,
    },
    /// Start containers.
    Start {
        #[arg(required = true)]
        containers: Vec<String>,
    },
    /// Stop containers.
    Stop {
        #[arg(required = true)]
        containers: Vec<String>,
    },
    /// Restart containers.
    Restart {
        #[arg(required = true)]
        containers: Vec<String>,
    },
    /// Pause containers.
    Pause {
        #[arg(required = true)]
        containers: Vec<String>,
    },
    /// Resume paused containers.
    Unpause {
        #[arg(required = true)]
        containers: Vec<String>,
    },
    /// Remove stopped containers.
    Rm {
        #[arg(required = true)]
        containers: Vec<String>,
        /// Stop them first if they run.
        #[arg(short, long)]
        force: bool,
    },
    /// Show a container's logs.
    Logs {
        container: String,
        /// How many of the last lines to show.
        #[arg(short = 'n', long, default_value_t = 100)]
        tail: usize,
        /// Keep printing new output.
        #[arg(short, long)]
        follow: bool,
        /// Show when each line was written.
        #[arg(short, long)]
        timestamps: bool,
    },
    /// Show what matters about a container: image, environment, ports,
    /// mounts, restart policy, health.
    Inspect {
        container: String,
        /// Show secret-looking environment values too.
        #[arg(long)]
        show_secrets: bool,
        /// Print Docker's full inspect output instead.
        #[arg(long)]
        raw: bool,
    },
    /// List images.
    Images,
    /// Pull an image, e.g. nginx:alpine.
    Pull { image: String },
    /// Build an image from a folder with a Dockerfile, with the engine's
    /// own builder (no Docker CLI needed). Honours `.dockerignore`.
    Build {
        /// The build context.
        #[arg(default_value = ".")]
        path: std::path::PathBuf,
        /// Name the image, e.g. my-app:1.0.
        #[arg(short, long)]
        tag: String,
        /// The Dockerfile, relative to the context.
        #[arg(short, long, default_value = "Dockerfile")]
        file: String,
        /// KEY=value; repeat for more.
        #[arg(long = "build-arg", value_name = "KEY=VALUE")]
        build_args: Vec<String>,
        /// The stage to stop at.
        #[arg(long, default_value = "")]
        target: String,
        #[arg(long)]
        no_cache: bool,
    },
    /// List volumes.
    Volumes,
    /// List networks.
    Networks,
    /// List Docker Compose projects.
    Projects,
    /// Show disk usage: images, containers, volumes and build cache.
    Df,
    /// Clean up what isn't used, and show the space it freed. `all` is
    /// everything but volumes, which hold data.
    Clean {
        #[arg(
            required = true,
            value_name = "WHAT",
            value_parser = PossibleValuesParser::new(CLEAN_TARGETS.map(|(name, _)| name))
        )]
        what: Vec<String>,
    },
    /// Remove every container, image and network, the build cache and the
    /// volumes: a clean start. Needs --yes.
    Reset {
        /// Keep the volumes, and the data in them.
        #[arg(long)]
        keep_volumes: bool,
        /// Really do it; without it, only says what would go.
        #[arg(long)]
        yes: bool,
    },
    /// Run Docker Compose on a project.
    Compose {
        #[command(subcommand)]
        command: ComposeCommand,
    },
    /// Run a popular service with settings for local development (postgres,
    /// redis…). Without a name, lists them.
    Quickstart {
        template: Option<String>,
        /// Container name (default: the template's, made unique).
        #[arg(long)]
        name: Option<String>,
        /// Image tag, e.g. 16.
        #[arg(long = "version", value_name = "TAG")]
        tag: Option<String>,
        /// Host port; repeat for templates with several ports.
        #[arg(long = "port", value_name = "PORT")]
        ports: Vec<u16>,
        /// A setting, e.g. POSTGRES_PASSWORD=secret; repeatable.
        #[arg(long = "set", value_name = "KEY=VALUE", value_parser = key_value)]
        values: Vec<(String, String)>,
        /// Don't keep the data in a volume.
        #[arg(long)]
        no_data: bool,
    },
    /// List engines and show which one is used.
    Engine {
        #[command(subcommand)]
        command: Option<EngineCommand>,
    },
}

#[derive(Debug, Subcommand)]
pub enum EngineCommand {
    /// Start the engine (Ferrodock's own by opening Ferrodock) and wait
    /// until it answers.
    Start,
}

#[derive(Debug, Subcommand)]
pub enum ComposeCommand {
    /// Create and start the services, in the background.
    Up {
        /// The Compose file, or the folder it's in.
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    /// Stop and remove the services' containers and networks. Volumes stay.
    Down {
        /// The Compose file, or the folder it's in.
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    /// Restart the services.
    Restart {
        /// The Compose file, or the folder it's in.
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    /// Show the services' logs.
    Logs {
        /// The Compose file, or the folder it's in.
        #[arg(default_value = ".")]
        path: PathBuf,
        /// How many of the last lines of each service to show.
        #[arg(short = 'n', long, default_value_t = 100)]
        tail: usize,
        /// Keep printing new output.
        #[arg(short, long)]
        follow: bool,
    },
}

fn key_value(text: &str) -> Result<(String, String), String> {
    text.split_once('=')
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .ok_or_else(|| format!("expected KEY=VALUE, got {text}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_like_the_docker_command() {
        let cli = Cli::parse_from(["fdock", "ps", "-a", "--json"]);
        assert!(cli.json);
        assert!(matches!(cli.command, Command::Ps { all: true }));
        let cli = Cli::parse_from(["fdock", "--engine", "work", "logs", "web", "-n", "5", "-f"]);
        assert_eq!(cli.engine.as_deref(), Some("work"));
        assert!(matches!(
            cli.command,
            Command::Logs {
                tail: 5,
                follow: true,
                ..
            }
        ));
        let cli = Cli::parse_from([
            "fdock",
            "quickstart",
            "postgres",
            "--port",
            "6543",
            "--set",
            "POSTGRES_PASSWORD=x=y",
        ]);
        let Command::Quickstart { ports, values, .. } = cli.command else {
            panic!()
        };
        assert_eq!(ports, [6543]);
        assert_eq!(values, [("POSTGRES_PASSWORD".to_owned(), "x=y".to_owned())]);
        assert!(Cli::try_parse_from(["fdock", "stop"]).is_err());
        Cli::command().debug_assert();
    }

    #[test]
    fn parses_engine_clean_reset_and_compose() {
        let cli = Cli::parse_from(["fdock", "engine"]);
        assert!(matches!(cli.command, Command::Engine { command: None }));
        let cli = Cli::parse_from(["fdock", "engine", "start", "--engine", "work"]);
        assert!(matches!(
            cli.command,
            Command::Engine {
                command: Some(EngineCommand::Start)
            }
        ));
        assert_eq!(cli.engine.as_deref(), Some("work"));

        let cli = Cli::parse_from(["fdock", "clean", "images", "build-cache", "--json"]);
        let Command::Clean { what } = cli.command else {
            panic!()
        };
        assert_eq!(what, ["images", "build-cache"]);
        assert!(Cli::try_parse_from(["fdock", "clean"]).is_err());
        assert!(Cli::try_parse_from(["fdock", "clean", "everything"]).is_err());

        let cli = Cli::parse_from(["fdock", "reset", "--keep-volumes"]);
        assert!(matches!(
            cli.command,
            Command::Reset {
                keep_volumes: true,
                yes: false
            }
        ));

        let cli = Cli::parse_from(["fdock", "compose", "up"]);
        let Command::Compose {
            command: ComposeCommand::Up { path },
        } = cli.command
        else {
            panic!()
        };
        assert_eq!(path, PathBuf::from("."));
        let cli = Cli::parse_from(["fdock", "compose", "logs", "shop/compose.yaml", "-f"]);
        assert!(matches!(
            cli.command,
            Command::Compose {
                command: ComposeCommand::Logs {
                    tail: 100,
                    follow: true,
                    ..
                }
            }
        ));
        assert!(Cli::try_parse_from(["fdock", "compose", "build"]).is_err());
    }

    use clap::CommandFactory;
}
