//! `fdock quickstart`: popular services with settings for local
//! development, the same templates as the app's Quick start page.

use ferrodock_core::summary::{Started, Template};
use ferrodock_core::{Overrides, TEMPLATES, format_size, template};

use super::Ctx;
use crate::output;

pub async fn run(ctx: &Ctx, name: Option<&str>, overrides: &Overrides) -> Result<(), String> {
    let Some(name) = name else {
        return list(ctx.json);
    };
    let template = template(name).ok_or_else(|| {
        let names: Vec<String> = TEMPLATES.iter().map(|t| t.slug()).collect();
        format!("No template called {name}. Templates: {}", names.join(", "))
    })?;
    let containers = ctx.conn.client.list_containers().await?;
    let choices = template.customize(template.defaults_for(&containers), overrides)?;
    let mut progress = output::Progress::new();
    ctx.conn
        .client
        .run_template(template, &choices, |p| {
            if !ctx.json && p.total > 0 {
                progress.show(format!(
                    "Pulling {}: {} of {}",
                    template.reference(&choices),
                    format_size(p.done as i64),
                    format_size(p.total as i64)
                ));
            }
        })
        .await?;
    progress.done();
    let started = Started::new(template, &choices);
    if ctx.json {
        return output::json(&started);
    }
    println!("Started {} ({})", started.name, started.image);
    println!("{}", started.connection);
    Ok(())
}

fn list(json: bool) -> Result<(), String> {
    let templates: Vec<Template> = TEMPLATES.iter().map(Template::from).collect();
    if json {
        return output::json(&templates);
    }
    let rows: Vec<Vec<String>> = templates
        .iter()
        .map(|t| {
            let ports: Vec<String> = t.ports.iter().map(u16::to_string).collect();
            vec![
                t.name.clone(),
                t.image.clone(),
                ports.join(", "),
                t.summary.clone(),
            ]
        })
        .collect();
    output::print_table(&["NAME", "IMAGE", "PORTS", "WHAT"], &rows, "");
    println!("\nRun one with `fdock quickstart NAME`; see `fdock quickstart --help` for options.");
    Ok(())
}
