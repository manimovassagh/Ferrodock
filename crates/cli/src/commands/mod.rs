//! What each command does. Every command but `engine` first connects to
//! an engine (see [`ferrodock_core::connect`]).

mod containers;
mod engine;
mod quickstart;
mod resources;

use ferrodock_core::connect::{Connection, connect};
use ferrodock_core::{Action, Overrides, Resource};

use crate::args::{Cli, Command};

/// What every command gets: the engine, and how to print.
pub struct Ctx {
    pub conn: Connection,
    pub json: bool,
}

pub async fn run(cli: Cli) -> Result<(), String> {
    if let Command::Engine = cli.command {
        return engine::show(cli.engine.as_deref(), cli.json).await;
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
        Command::Volumes => resources::volumes(&ctx).await,
        Command::Networks => resources::networks(&ctx).await,
        Command::Projects => resources::projects(&ctx).await,
        Command::Df => resources::df(&ctx).await,
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
        Command::Engine => unreachable!("handled above"),
    }
}
