//! `fdock`: Ferrodock on the command line. The same engines, containers,
//! images and Quick start templates as the app, for the terminal, scripts
//! and AI tools (`--json` prints compact JSON).

mod args;
mod commands;
mod output;

use std::process::ExitCode;

use clap::Parser;

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let cli = args::Cli::parse();
    match commands::run(cli).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("fdock: {e}");
            ExitCode::FAILURE
        }
    }
}
