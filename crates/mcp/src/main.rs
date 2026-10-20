//! `ferrodock-mcp`: an MCP server, over standard input and output, that
//! lets AI tools see and manage Docker containers, images and Quick start
//! services, on Ferrodock's own engine or any other. Built on
//! ferrodock-core, like the app and the `fdock` CLI.
//!
//! Options:
//!   --engine NAME   the engine to use (by default: DOCKER_HOST when set,
//!                   else Ferrodock's own engine when it runs, else the
//!                   current Docker context)
//!   --read-only     only the tools that look; none that change anything

mod tools;

use std::process::ExitCode;

use rmcp::ServiceExt;

const HELP: &str = "\
ferrodock-mcp: an MCP server for Docker, over standard input and output.

Usage: ferrodock-mcp [--engine NAME] [--read-only]

  --engine NAME  The engine to use. By default: DOCKER_HOST when set, else
                 Ferrodock's own engine when it runs, else the current
                 Docker context.
  --read-only    Only the tools that look; none that change anything.

Add it to an AI tool, for example: claude mcp add ferrodock -- ferrodock-mcp";

#[derive(Debug, Default, PartialEq, Eq)]
struct Options {
    engine: Option<String>,
    read_only: bool,
}

/// Reads the options; `Err` carries what to print and exit with.
fn options(args: impl IntoIterator<Item = String>) -> Result<Options, (String, bool)> {
    let mut options = Options::default();
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--engine" => {
                options.engine = Some(
                    args.next()
                        .ok_or_else(|| ("--engine needs a name".to_owned(), false))?,
                );
            }
            "--read-only" => options.read_only = true,
            "-h" | "--help" => return Err((HELP.to_owned(), true)),
            "-V" | "--version" => {
                return Err((format!("ferrodock-mcp {}", env!("CARGO_PKG_VERSION")), true));
            }
            other => return Err((format!("unknown option {other}\n\n{HELP}"), false)),
        }
    }
    Ok(options)
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let options = match options(std::env::args().skip(1)) {
        Ok(options) => options,
        Err((text, ok)) => {
            if ok {
                println!("{text}");
                return ExitCode::SUCCESS;
            }
            eprintln!("ferrodock-mcp: {text}");
            return ExitCode::from(2);
        }
    };
    let server = tools::Ferrodock::new(options.engine, options.read_only);
    // Standard output carries the protocol; anything else goes to stderr.
    match server.serve(rmcp::transport::stdio()).await {
        Ok(running) => {
            let _ = running.waiting().await;
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("ferrodock-mcp: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_its_options() {
        let args = |list: &[&str]| options(list.iter().map(|s| (*s).to_owned()));
        assert_eq!(args(&[]), Ok(Options::default()));
        assert_eq!(
            args(&["--read-only", "--engine", "work"]),
            Ok(Options {
                engine: Some("work".into()),
                read_only: true
            })
        );
        assert!(matches!(args(&["--engine"]), Err((_, false))));
        assert!(matches!(args(&["--help"]), Err((_, true))));
        assert!(matches!(args(&["--nope"]), Err((_, false))));
    }
}
