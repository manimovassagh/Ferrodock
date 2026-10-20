//! Reaching an engine from outside the app: the `fdock` CLI and the MCP
//! server pick one the way Ferrodock does, and check that it answers.

use std::time::Duration;

use super::{DockerClient, Engine, builtin, engines, pick};

/// How long an engine gets to answer before the next one is tried.
const PATIENCE: Duration = Duration::from_secs(3);

/// An engine that answered.
#[derive(Clone)]
pub struct Connection {
    pub engine: Engine,
    pub client: DockerClient,
    /// The engine's Docker version, e.g. `28.5.2`.
    pub version: String,
}

/// The engines to try, in order: the one called `name`; else the default
/// one when `DOCKER_HOST` is set; else Ferrodock's own engine, then the
/// Docker CLI's current context.
pub fn candidates(
    list: &[Engine],
    name: Option<&str>,
    docker_host: bool,
) -> Result<Vec<Engine>, String> {
    if let Some(name) = name {
        return list
            .iter()
            .find(|e| e.name == name)
            .map(|e| vec![e.clone()])
            .ok_or_else(|| {
                let names: Vec<&str> = list.iter().map(|e| e.name.as_str()).collect();
                format!("No engine called {name}. Engines: {}", names.join(", "))
            });
    }
    if docker_host {
        return Ok(pick(list, Some("default")).into_iter().collect());
    }
    let mut found: Vec<Engine> = list
        .iter()
        .filter(|e| e.name == builtin::NAME)
        .cloned()
        .collect();
    if let Some(current) = pick(list, None)
        && !found.contains(&current)
    {
        found.push(current);
    }
    Ok(found)
}

/// Connects to the engine called `name`, or to the first engine that
/// answers (see [`candidates`]).
pub async fn connect(name: Option<&str>) -> Result<Connection, String> {
    let list = engines();
    let docker_host = std::env::var_os("DOCKER_HOST").is_some();
    let mut error = "No Docker engine found.".to_owned();
    for engine in candidates(&list, name, docker_host)? {
        let client = match DockerClient::connect_to(&engine) {
            Ok(client) => client,
            Err(e) => {
                error = e;
                continue;
            }
        };
        match tokio::time::timeout(PATIENCE, client.version()).await {
            Ok(Ok(version)) => {
                return Ok(Connection {
                    engine,
                    client,
                    version,
                });
            }
            Ok(Err(e)) => error = format!("{}: {e}", engine.name),
            Err(_) => error = format!("{} didn't answer.", engine.name),
        }
    }
    Err(error)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine(name: &str, current: bool) -> Engine {
        Engine {
            name: name.into(),
            host: format!("unix:///{name}.sock"),
            tls: None,
            current,
        }
    }

    #[test]
    fn tries_ferrodock_first_then_the_current_context() {
        let list = [
            engine("default", false),
            engine("work", true),
            engine(builtin::NAME, false),
        ];
        let names = |found: Vec<Engine>| found.into_iter().map(|e| e.name).collect::<Vec<_>>();
        assert_eq!(
            names(candidates(&list, None, false).unwrap()),
            [builtin::NAME, "work"]
        );
        // DOCKER_HOST wins, as it does for the docker command.
        assert_eq!(names(candidates(&list, None, true).unwrap()), ["default"]);
        assert_eq!(
            names(candidates(&list, Some("work"), true).unwrap()),
            ["work"]
        );
        let missing = candidates(&list, Some("nope"), false).unwrap_err();
        assert!(missing.contains("default, work, Ferrodock"), "{missing}");
    }
}
