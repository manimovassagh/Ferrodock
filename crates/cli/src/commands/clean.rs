//! `clean` and `reset`: freeing space, and starting over.

use ferrodock_core::summary::Cleaned;
use ferrodock_core::{clean_kinds, format_size};

use super::Ctx;
use crate::output;

pub async fn clean(ctx: &Ctx, what: &[String]) -> Result<(), String> {
    let kinds = clean_kinds(what)?;
    let cleaned = Cleaned::new(&ctx.conn.client.clean(&kinds).await?);
    if ctx.json {
        return output::json(&cleaned);
    }
    for part in &cleaned.cleaned {
        println!("Cleaned up {}: {} freed", part.what, part.freed);
    }
    if cleaned.cleaned.len() > 1 {
        println!("{} freed in all", cleaned.freed);
    }
    Ok(())
}

/// What a reset removes, for the refusal without `--yes`.
fn what_goes(keep_volumes: bool) -> &'static str {
    if keep_volumes {
        "every container, image and network, and the build cache (volumes stay)"
    } else {
        "every container, image, network and volume, with the data in them, and the build cache"
    }
}

/// Resets the engine, only with `yes`: `fdock` never asks, so a script
/// can't reset by accident.
pub async fn reset(ctx: &Ctx, keep_volumes: bool, yes: bool) -> Result<(), String> {
    let engine = &ctx.conn.engine.name;
    if !yes {
        let again = if keep_volumes {
            "fdock reset --keep-volumes --yes"
        } else {
            "fdock reset --yes"
        };
        return Err(format!(
            "This removes {} from {engine}. To do it: {again}",
            what_goes(keep_volumes)
        ));
    }
    let freed = ctx.conn.client.reset(keep_volumes).await?;
    if ctx.json {
        return output::json(&serde_json::json!({
            "reset": engine,
            "freed": format_size(freed),
            "volumes_kept": keep_volumes,
        }));
    }
    println!("Reset {engine}: {} freed", format_size(freed));
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn says_whether_volumes_go() {
        assert!(super::what_goes(true).contains("volumes stay"));
        assert!(super::what_goes(false).contains("volume, with the data"));
    }
}
