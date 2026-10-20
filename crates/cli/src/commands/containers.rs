//! `ps`, `start`/`stop`/… , `logs` and `inspect`.

use std::collections::BTreeMap;
use std::io::Write;

use ferrodock_core::summary::{Container, Details};
use ferrodock_core::{Action, Resource};
use futures_util::StreamExt;
use serde::Serialize;

use super::Ctx;
use crate::output;

pub async fn ps(ctx: &Ctx, all: bool) -> Result<(), String> {
    let containers = ctx.conn.client.list_containers().await?;
    let shown: Vec<Container> = containers
        .iter()
        .filter(|c| all || c.is_active())
        .map(Container::from)
        .collect();
    if ctx.json {
        return output::json(&shown);
    }
    let rows: Vec<Vec<String>> = shown
        .iter()
        .map(|c| {
            vec![
                c.name.clone(),
                c.image.clone(),
                c.status.clone(),
                c.ports.join(", "),
            ]
        })
        .collect();
    let empty = if all {
        "No containers."
    } else {
        "No running containers. `fdock ps -a` shows stopped ones too."
    };
    output::print_table(&["NAME", "IMAGE", "STATUS", "PORTS"], &rows, empty);
    Ok(())
}

#[derive(Serialize)]
struct Outcome {
    done: Vec<String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    failed: BTreeMap<String, String>,
}

/// Runs `action` on each container, then says how it went. Fails when any
/// of them failed.
pub async fn act(ctx: &Ctx, action: Action, containers: &[String]) -> Result<(), String> {
    let mut outcome = Outcome {
        done: Vec::new(),
        failed: BTreeMap::new(),
    };
    for name in containers {
        match ctx.conn.client.perform(action, name).await {
            Ok(_) => {
                if !ctx.json {
                    println!("{} {name}", action.past_tense());
                }
                outcome.done.push(name.clone());
            }
            Err(e) => {
                if !ctx.json {
                    eprintln!("{name}: {e}");
                }
                outcome.failed.insert(name.clone(), e);
            }
        }
    }
    if ctx.json {
        output::json(&outcome)?;
    }
    match outcome.failed.len() {
        0 => Ok(()),
        n => Err(format!("{n} of {} failed", containers.len())),
    }
}

pub async fn logs(
    ctx: &Ctx,
    container: &str,
    tail: usize,
    follow: bool,
    timestamps: bool,
) -> Result<(), String> {
    if ctx.json && follow {
        return Err("--json can't follow; leave out -f to get the lines".into());
    }
    let mut stream = Box::pin(
        ctx.conn
            .client
            .logs_with(container, tail, follow, timestamps),
    );
    if ctx.json {
        let mut text = String::new();
        while let Some(chunk) = stream.next().await {
            text.push_str(&chunk?);
        }
        let lines: Vec<&str> = text.lines().collect();
        return output::json(&serde_json::json!({ "lines": lines }));
    }
    let mut out = std::io::stdout().lock();
    while let Some(chunk) = stream.next().await {
        // A closed pipe (`fdock logs web | head`) just ends the output.
        if out
            .write_all(chunk?.as_bytes())
            .and_then(|()| out.flush())
            .is_err()
        {
            break;
        }
    }
    Ok(())
}

pub async fn inspect(
    ctx: &Ctx,
    container: &str,
    show_secrets: bool,
    raw: bool,
) -> Result<(), String> {
    let inspection = ctx
        .conn
        .client
        .inspect(Resource::Containers, container)
        .await?;
    if raw {
        println!("{}", inspection.json);
        return Ok(());
    }
    let details = Details::new(&inspection, show_secrets);
    if ctx.json {
        return output::json(&details);
    }
    let mut rows: Vec<(&str, Vec<String>)> = vec![
        ("id", vec![details.id.clone()]),
        ("image", vec![details.image.clone()]),
        ("command", vec![details.command.clone()]),
        ("ports", details.ports.clone()),
        ("mounts", details.mounts.clone()),
        ("network", vec![details.network.clone()]),
        ("restart", details.restart.clone().into_iter().collect()),
        ("health", details.health.clone().into_iter().collect()),
        ("env", details.env.clone()),
        ("labels", details.labels.clone()),
    ];
    rows.retain(|(_, values)| values.iter().any(|v| !v.is_empty()));
    for (key, values) in rows {
        for (i, value) in values.iter().enumerate() {
            println!("{:<8}  {value}", if i == 0 { key } else { "" });
        }
    }
    Ok(())
}
