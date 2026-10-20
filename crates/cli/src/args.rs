//! What `fdock` takes on the command line.

use clap::{Parser, Subcommand};

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
    /// List volumes.
    Volumes,
    /// List networks.
    Networks,
    /// List Docker Compose projects.
    Projects,
    /// Show disk usage: images, containers, volumes and build cache.
    Df,
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
    Engine,
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

    use clap::CommandFactory;
}
