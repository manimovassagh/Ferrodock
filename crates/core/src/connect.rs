//! Reaching an engine from outside the app: the `fdock` CLI and the MCP
//! server pick one the way Ferrodock does, check that it answers, and
//! start it when it doesn't.

use std::time::{Duration, Instant};

use super::{DockerClient, Engine, Starter, builtin, engines, pick, starter_outside_app};

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

/// How long a started engine gets to answer. Engines that run in a VM can
/// take a minute or two to boot it.
const BOOT_PATIENCE: Duration = Duration::from_secs(180);

/// Starts the engine called `name`, or the first one [`connect`] tries,
/// unless one answers already, and waits for it to answer. `starting` is
/// told what starts it. Returns the engine, and whether it was started.
pub async fn start(
    name: Option<&str>,
    starting: impl FnOnce(&Starter),
) -> Result<(Connection, bool), String> {
    if let Ok(conn) = connect(name).await {
        return Ok((conn, false));
    }
    let list = engines();
    let docker_host = std::env::var_os("DOCKER_HOST").is_some();
    let engine = candidates(&list, name, docker_host)?
        .into_iter()
        .next()
        .ok_or("No Docker engine found.")?;
    let Some(starter) = starter_outside_app(&engine) else {
        return Err(nothing_starts(&engine.name));
    };
    starting(&starter);
    let run = starter.clone();
    tokio::task::spawn_blocking(move || run.run())
        .await
        .map_err(|e| e.to_string())??;
    let since = Instant::now();
    loop {
        match connect(Some(&engine.name)).await {
            Ok(conn) => return Ok((conn, true)),
            Err(_) if since.elapsed() < BOOT_PATIENCE => {
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            Err(e) => {
                let mut error = format!("{} didn't answer after 3 minutes: {e}", engine.name);
                if let Starter::Ferrodock(_) = starter {
                    error.push_str(
                        "\nWhen Ferrodock was open already, or \"Start the Docker engine when \
                         Ferrodock opens\" is off in its Settings, start the engine from Ferrodock.",
                    );
                }
                return Err(error);
            }
        }
    }
}

/// Why nothing can start the engine called `name`, and what to do instead.
fn nothing_starts(name: &str) -> String {
    if name == builtin::NAME {
        return "Couldn't find the Ferrodock app to start its engine: open Ferrodock.".into();
    }
    let mut error =
        format!("Ferrodock doesn't know how to start {name} on this computer: start it yourself");
    if builtin::SUPPORTED {
        error.push_str(", or open Ferrodock to set up its own engine");
    }
    error.push('.');
    error
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
