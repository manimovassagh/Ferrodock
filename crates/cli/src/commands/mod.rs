//! What each command does. Every command but `engine` first connects to
//! an engine (see [`ferrodock_core::connect`]).

mod clean;
mod compose;
mod containers;
mod engine;
mod quickstart;
mod resources;

use ferrodock_core::connect::{Connection, connect};
use ferrodock_core::{Action, ComposeAction, Overrides, Resource};

use crate::args::{Cli, Command, ComposeCommand, EngineCommand};

/// What every command gets: the engine, and how to print.
pub struct Ctx {
    pub conn: Connection,
    pub json: bool,
}

pub async fn run(cli: Cli) -> Result<(), String> {
    if let Command::Engine { command } = &cli.command {
        return match command {
            None => engine::show(cli.engine.as_deref(), cli.json).await,
            Some(EngineCommand::Start) => engine::start(cli.engine.as_deref(), cli.json).await,
        };
    }
    let conn = connect(cli.engine.as_deref())
        .await
        .map_err(|e| format!("{e}\n`fdock engine` lists the engines and whether they answer."))?;
    let ctx = Ctx {
        conn,
        json: cli.json,
    };
    match cli.command {
        Command::Ps { all } => containers::ps(&ctx, all).await,
        Command::Start { containers } => containers::act(&ctx, Action::Start, &containers).await,
        Command::Stop { containers } => containers::act(&ctx, Action::Stop, &containers).await,
        Command::Restart { containers } => {
            containers::act(&ctx, Action::Restart, &containers).await
        }
        Command::Pause { containers } => containers::act(&ctx, Action::Pause, &containers).await,
        Command::Unpause { containers } => {
            containers::act(&ctx, Action::Unpause, &containers).await
        }
        Command::Rm { containers, force } => {
            let action = if force {
                Action::StopAndRemove
            } else {
                Action::Remove(Resource::Containers)
            };
            containers::act(&ctx, action, &containers).await
        }
        Command::Logs {
            container,
            tail,
            follow,
            timestamps,
        } => containers::logs(&ctx, &container, tail, follow, timestamps).await,
        Command::Inspect {
            container,
            show_secrets,
            raw,
        } => containers::inspect(&ctx, &container, show_secrets, raw).await,
        Command::Images => resources::images(&ctx).await,
        Command::Pull { image } => resources::pull(&ctx, &image).await,
        Command::Build {
            path,
            tag,
            file,
            build_args,
            target,
            no_cache,
        } => {
            let args = build_args
                .iter()
                .map(|a| {
                    a.split_once('=')
                        .map(|(k, v)| (k.to_owned(), v.to_owned()))
                        .ok_or_else(|| format!("--build-arg {a}: write it as KEY=value"))
                })
                .collect::<Result<_, _>>()?;
            let spec = ferrodock_core::BuildSpec {
                context: path,
                dockerfile: file,
                tag,
                args,
                target,
                no_cache,
                ..Default::default()
            };
            resources::build(&ctx, &spec).await
        }
        Command::Volumes => resources::volumes(&ctx).await,
        Command::Networks => resources::networks(&ctx).await,
        Command::Projects => resources::projects(&ctx).await,
        Command::Df => resources::df(&ctx).await,
        Command::Clean { what } => clean::clean(&ctx, &what).await,
        Command::Reset { keep_volumes, yes } => clean::reset(&ctx, keep_volumes, yes).await,
        Command::Compose { command } => match command {
            ComposeCommand::Up { path } => compose::run(&ctx, &path, ComposeAction::Up).await,
            ComposeCommand::Down { path } => compose::run(&ctx, &path, ComposeAction::Down).await,
            ComposeCommand::Restart { path } => {
                compose::run(&ctx, &path, ComposeAction::Restart).await
            }
            ComposeCommand::Logs { path, tail, follow } => {
                compose::logs(&ctx, &path, tail, follow).await
            }
        },
        Command::Quickstart {
            template,
            name,
            tag,
            ports,
            values,
            no_data,
        } => {
            let overrides = Overrides {
                name,
                tag,
                ports,
                values,
                keep_data: no_data.then_some(false),
            };
            quickstart::run(&ctx, template.as_deref(), &overrides).await
        }
        Command::Engine { .. } => unreachable!("handled above"),
    }
}
