//! `fdock engine`: the engines Ferrodock knows of, and which one answers.

use ferrodock_core::connect::connect;
use ferrodock_core::engines;
use ferrodock_core::summary::Engine;
use serde::Serialize;

use crate::output;

#[derive(Serialize)]
struct Report {
    engines: Vec<Engine>,
    /// Why no engine answered.
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

pub async fn show(name: Option<&str>, json: bool) -> Result<(), String> {
    let picked = connect(name).await;
    let engines: Vec<Engine> = engines()
        .iter()
        .map(|e| {
            let using = picked.as_ref().ok().filter(|c| c.engine.name == e.name);
            Engine {
                name: e.name.clone(),
                host: e.host.clone(),
                in_use: using.is_some(),
                version: using.map(|c| c.version.clone()),
            }
        })
        .collect();
    let error = picked.as_ref().err().cloned();
    if json {
        return output::json(&Report { engines, error });
    }
    let rows: Vec<Vec<String>> = engines
        .iter()
        .map(|e| {
            vec![
                if e.in_use { "*" } else { "" }.to_owned(),
                e.name.clone(),
                e.host.clone(),
            ]
        })
        .collect();
    output::print_table(&["", "NAME", "HOST"], &rows, "No engines.");
    match &picked {
        Ok(c) => println!("\nUsing {} (Docker {}).", c.engine.name, c.version),
        Err(e) => {
            println!("\nNo engine answered: {e}");
            println!("Start Ferrodock (or your Docker engine), or pick one with --engine NAME.");
        }
    }
    Ok(())
}
