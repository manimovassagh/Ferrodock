//! `compose up`, `down`, `restart` and `logs`: Docker Compose (see
//! [`ferrodock_core::compose_cli`]), aimed at the engine in use.

use std::io::Write;
use std::path::Path;

use ferrodock_core::ComposeAction;
use ferrodock_core::compose_cli::{command, project_args, steps, stream, target_at};
use ferrodock_core::compose_install;
use tokio::process::Command;
use tokio::sync::Notify;

use super::Ctx;
use crate::output;

/// Gets Ferrodock's copy of Compose first when there's no other.
async fn ensure(ctx: &Ctx) -> Result<(), String> {
    compose_install::ensure(|| {
        if !ctx.json {
            eprintln!(
                "Downloading Docker Compose {} (once)…",
                compose_install::VERSION
            );
        }
    })
    .await
    .map_err(|e| format!("{e}\nInstall Docker Compose, or try again."))
}

/// Runs `cmd`, printing what it prints. Returns whether it succeeded; a
/// closed pipe (`fdock compose logs -f | head`) just ends it.
async fn print(cmd: Command) -> Result<bool, String> {
    let closed = Notify::new();
    let mut out = std::io::stdout().lock();
    let lines = stream(cmd, |line| {
        if writeln!(out, "{line}").and_then(|()| out.flush()).is_err() {
            closed.notify_one();
        }
    });
    tokio::select! {
        ok = lines => ok,
        // Dropping the command stops it.
        () = closed.notified() => Ok(true),
    }
}

/// Runs the commands for `action` on the project at `path`.
pub async fn run(ctx: &Ctx, path: &Path, action: ComposeAction) -> Result<(), String> {
    let target = target_at(path)?;
    ensure(ctx).await?;
    let mut lines = Vec::new();
    let mut ok = true;
    for args in steps(&target, &action) {
        let cmd = command(Some(&ctx.conn.engine), Some(&target.dir), &args);
        ok = if ctx.json {
            stream(cmd, |line| lines.push(line)).await?
        } else {
            print(cmd).await?
        };
        if !ok {
            break;
        }
    }
    let (verb, done) = match action {
        ComposeAction::Down => ("down", "Stopped and removed"),
        ComposeAction::Restart => ("restart", "Restarted"),
        _ => ("up", "Started"),
    };
    if ctx.json {
        output::json(&serde_json::json!({
            "project": target.label(),
            "action": verb,
            "ok": ok,
            "output": lines,
        }))?;
    }
    if !ok {
        return Err("docker compose failed".into());
    }
    if !ctx.json {
        println!("{done} {}", target.label());
    }
    Ok(())
}

pub async fn logs(ctx: &Ctx, path: &Path, tail: usize, follow: bool) -> Result<(), String> {
    if ctx.json && follow {
        return Err("--json can't follow; leave out -f to get the lines".into());
    }
    let target = target_at(path)?;
    ensure(ctx).await?;
    let mut args = project_args(&target);
    args.extend(["logs".into(), "--tail".into(), tail.to_string()]);
    if follow {
        args.push("-f".into());
    }
    let cmd = command(Some(&ctx.conn.engine), Some(&target.dir), &args);
    let ok = if ctx.json {
        let mut lines = Vec::new();
        let ok = stream(cmd, |line| lines.push(line)).await?;
        if ok {
            output::json(&serde_json::json!({ "lines": lines }))?;
        } else {
            eprintln!("{}", lines.join("\n"));
        }
        ok
    } else {
        print(cmd).await?
    };
    if ok {
        Ok(())
    } else {
        Err("docker compose failed".into())
    }
}
