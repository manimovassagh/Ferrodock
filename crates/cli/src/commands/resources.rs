//! `images`, `pull`, `build`, `volumes`, `networks`, `projects` and `df`.

use ferrodock_core::summary::{Disk, Image, Network, Project, Space, Volume};
use ferrodock_core::{format_size, group_projects};

use super::Ctx;
use crate::output;

pub async fn images(ctx: &Ctx) -> Result<(), String> {
    let images: Vec<Image> = ctx
        .conn
        .client
        .list_images()
        .await?
        .iter()
        .map(Image::from)
        .collect();
    if ctx.json {
        return output::json(&images);
    }
    let rows: Vec<Vec<String>> = images
        .iter()
        .map(|i| {
            vec![
                i.name.clone(),
                i.id.clone(),
                i.size.clone(),
                i.created.clone(),
            ]
        })
        .collect();
    output::print_table(&["NAME", "ID", "SIZE", "CREATED"], &rows, "No images.");
    Ok(())
}

/// Pulls `image`, showing progress on a terminal.
pub async fn pull(ctx: &Ctx, image: &str) -> Result<(), String> {
    let mut progress = output::Progress::new();
    ctx.conn
        .client
        .pull(image, |p| {
            if !ctx.json && p.total > 0 {
                progress.show(format!(
                    "Pulling {image}: {} of {}",
                    format_size(p.done as i64),
                    format_size(p.total as i64)
                ));
            }
        })
        .await?;
    progress.done();
    if ctx.json {
        return output::json(&serde_json::json!({ "pulled": image }));
    }
    println!("Pulled {image}");
    Ok(())
}

pub async fn build(ctx: &Ctx, spec: &ferrodock_core::BuildSpec) -> Result<(), String> {
    // The build's own output goes to stderr with --json, so stdout stays
    // one JSON value.
    ctx.conn
        .client
        .build_image(spec, |line| {
            if ctx.json {
                eprintln!("{line}");
            } else {
                println!("{line}");
            }
        })
        .await?;
    if ctx.json {
        return output::json(&serde_json::json!({ "built": spec.tag }));
    }
    Ok(())
}

pub async fn volumes(ctx: &Ctx) -> Result<(), String> {
    let volumes: Vec<Volume> = ctx
        .conn
        .client
        .list_volumes()
        .await?
        .iter()
        .map(Volume::from)
        .collect();
    if ctx.json {
        return output::json(&volumes);
    }
    let rows: Vec<Vec<String>> = volumes
        .iter()
        .map(|v| {
            vec![
                v.name.clone(),
                v.driver.clone(),
                v.created.clone().unwrap_or_default(),
            ]
        })
        .collect();
    output::print_table(&["NAME", "DRIVER", "CREATED"], &rows, "No volumes.");
    Ok(())
}

pub async fn networks(ctx: &Ctx) -> Result<(), String> {
    let networks: Vec<Network> = ctx
        .conn
        .client
        .list_networks()
        .await?
        .iter()
        .map(Network::from)
        .collect();
    if ctx.json {
        return output::json(&networks);
    }
    let rows: Vec<Vec<String>> = networks
        .iter()
        .map(|n| {
            let mut subnets = n.subnets.join(", ");
            if n.internal {
                subnets.push_str(" (internal)");
            }
            vec![n.name.clone(), n.id.clone(), n.driver.clone(), subnets]
        })
        .collect();
    output::print_table(&["NAME", "ID", "DRIVER", "SUBNETS"], &rows, "No networks.");
    Ok(())
}

pub async fn projects(ctx: &Ctx) -> Result<(), String> {
    let containers = ctx.conn.client.list_containers().await?;
    let projects: Vec<Project> = group_projects(&containers)
        .iter()
        .map(Project::from)
        .collect();
    if ctx.json {
        return output::json(&projects);
    }
    let rows: Vec<Vec<String>> = projects
        .iter()
        .map(|p| {
            vec![
                p.name.clone(),
                format!("{}/{} running", p.running, p.containers),
                p.services.join(", "),
                p.dir.clone(),
            ]
        })
        .collect();
    output::print_table(
        &["PROJECT", "STATUS", "SERVICES", "FOLDER"],
        &rows,
        "No Docker Compose projects.",
    );
    Ok(())
}

pub async fn df(ctx: &Ctx) -> Result<(), String> {
    let usage = ctx.conn.client.disk_usage().await?;
    if !usage.is_reported() {
        return Err(
            "This engine doesn't report disk usage (it needs Docker Engine 29 or newer)".into(),
        );
    }
    let disk = Disk::from(&usage);
    if ctx.json {
        return output::json(&disk);
    }
    let parts = [
        ("Images", &disk.images),
        ("Containers", &disk.containers),
        ("Volumes", &disk.volumes),
        ("Build cache", &disk.build_cache),
    ];
    let mut rows: Vec<Vec<String>> = parts
        .iter()
        .filter_map(|(kind, space): &(&str, &Option<Space>)| {
            let s = space.as_ref()?;
            Some(vec![
                (*kind).to_owned(),
                s.size.clone(),
                s.reclaimable.clone(),
                s.count.to_string(),
                s.in_use.to_string(),
            ])
        })
        .collect();
    rows.push(vec![
        "Total".into(),
        disk.total.clone(),
        disk.reclaimable.clone(),
        String::new(),
        String::new(),
    ]);
    output::print_table(
        &["TYPE", "SIZE", "RECLAIMABLE", "COUNT", "IN USE"],
        &rows,
        "",
    );
    Ok(())
}
