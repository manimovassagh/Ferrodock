//! `cloud start`, `stop`, `status`, `reset` and `env`: the local cloud
//! emulator on the command line.

use ferrodock_core::cloud::{self, CloudLabels, CloudSpec};
use serde_json::json;

use super::Ctx;
use crate::output;

/// The emulator's container: what it was made with, and whether it runs.
async fn emulator(ctx: &Ctx) -> Result<Option<(CloudLabels, bool)>, String> {
    let containers = ctx.conn.client.list_containers().await?;
    Ok(containers
        .iter()
        .find_map(|c| Some((c.cloud?, c.is_running()))))
}

/// The port of the running emulator, or why there's none.
async fn running_port(ctx: &Ctx) -> Result<u16, String> {
    match emulator(ctx).await? {
        Some((labels, true)) => Ok(labels.port),
        _ => Err("The cloud emulator isn't running. Start it with: fdock cloud start".into()),
    }
}

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|e| e.to_string())?
}

pub async fn start(ctx: &Ctx, port: u16, keep_data: bool) -> Result<(), String> {
    let spec = CloudSpec { port, keep_data };
    let mut progress = output::Progress::new();
    let started = ctx
        .conn
        .client
        .start_cloud(spec, |step| progress.show(step.to_owned()))
        .await;
    progress.done();
    started?;
    let endpoint = cloud::endpoint(port);
    if ctx.json {
        return output::json(
            &json!({ "running": true, "endpoint": endpoint, "data_kept": keep_data }),
        );
    }
    println!("The cloud emulator is running at {endpoint}");
    println!("Point your tools at it with: eval \"$(fdock cloud env)\"");
    Ok(())
}

pub async fn stop(ctx: &Ctx) -> Result<(), String> {
    ctx.conn.client.stop_cloud().await?;
    if ctx.json {
        return output::json(&json!({ "running": false }));
    }
    println!("Stopped the cloud emulator");
    Ok(())
}

pub async fn status(ctx: &Ctx) -> Result<(), String> {
    let Some((labels, true)) = emulator(ctx).await? else {
        if ctx.json {
            return output::json(&json!({ "running": false }));
        }
        println!("The cloud emulator isn't running. Start it with: fdock cloud start");
        return Ok(());
    };
    let port = labels.port;
    let info = blocking(move || cloud::info(port)).await?;
    let endpoint = cloud::endpoint(port);
    if ctx.json {
        let services: Vec<_> = info
            .services
            .iter()
            .map(|s| json!({ "name": s.name, "title": s.title, "operations": s.operations }))
            .collect();
        return output::json(&json!({
            "running": true,
            "endpoint": endpoint,
            "version": info.version,
            "data_kept": labels.keep_data,
            "services": services,
        }));
    }
    println!("Running at {endpoint} (ferrocloud {})", info.version);
    println!(
        "Data is {}",
        if labels.keep_data {
            "kept between starts"
        } else {
            "not kept: it starts empty each time"
        }
    );
    println!();
    let rows: Vec<Vec<String>> = info
        .services
        .iter()
        .map(|s| {
            vec![
                s.title.clone(),
                s.name.clone(),
                s.operations.len().to_string(),
            ]
        })
        .collect();
    println!(
        "{}",
        output::table(&["SERVICE", "NAME", "OPERATIONS"], &rows)
    );
    Ok(())
}

/// Empties the emulator, only with `yes`: `fdock` never asks, so a script
/// can't do it by accident.
pub async fn reset(ctx: &Ctx, yes: bool) -> Result<(), String> {
    let port = running_port(ctx).await?;
    if !yes {
        return Err(
            "This deletes everything created in the cloud emulator. To do it: fdock cloud reset --yes".into(),
        );
    }
    blocking(move || cloud::reset(port)).await?;
    if ctx.json {
        return output::json(&json!({ "reset": true }));
    }
    println!("Emptied the cloud emulator");
    Ok(())
}

/// The lines `env` prints, for a shell to run.
fn env_text(port: u16, powershell: bool) -> String {
    cloud::env_lines(port)
        .into_iter()
        .map(|(name, value)| {
            if powershell {
                format!("$env:{name} = \"{value}\"\n")
            } else {
                format!("export {name}={value}\n")
            }
        })
        .collect()
}

/// Prints the variables for the emulator where it runs, or where it
/// would by default: a shell's profile can hold this before it's started.
pub async fn env(ctx: &Ctx, powershell: bool) -> Result<(), String> {
    let port = emulator(ctx)
        .await?
        .map_or(cloud::CLOUD_PORT, |(labels, _)| labels.port);
    if ctx.json {
        let vars: serde_json::Map<String, serde_json::Value> = cloud::env_lines(port)
            .into_iter()
            .map(|(name, value)| (name.to_owned(), json!(value)))
            .collect();
        return output::json(&vars);
    }
    print!("{}", env_text(port, powershell));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prints_variables_for_each_shell() {
        let sh = env_text(4570, false);
        assert!(sh.starts_with("export AWS_ENDPOINT_URL=http://"), "{sh}");
        assert!(sh.contains(":4570\n") && sh.contains("export AWS_ACCESS_KEY_ID=test\n"));
        assert_eq!(sh.lines().count(), 4);
        let ps = env_text(4570, true);
        assert!(
            ps.contains("$env:AWS_DEFAULT_REGION = \"us-east-1\"\n"),
            "{ps}"
        );
    }
}
