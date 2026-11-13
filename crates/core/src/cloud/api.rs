//! The emulator's own API, under `/_ferrocloud/`: what it is and has, and
//! a reset. Plain HTTP on its published port.

use std::time::Duration;

use serde::Deserialize;

/// What a running emulator says about itself.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CloudInfo {
    /// The emulator's own version.
    pub version: String,
    pub services: Vec<CloudService>,
}

/// A service the emulator has, e.g. S3.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
pub struct CloudService {
    /// As tools name it, e.g. `s3`.
    pub name: String,
    /// As people name it, e.g. `S3`.
    pub title: String,
    /// The operations it supports, e.g. `CreateBucket`.
    pub operations: Vec<String>,
}

/// Where tools reach the emulator: `http://localhost:4566`.
pub fn endpoint(port: u16) -> String {
    format!("http://{}:{port}", crate::engine_host())
}

/// Straight to the emulator: it's on this computer (or the engine's), so
/// a proxy set for the internet is the wrong way there.
fn agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .proxy(None)
        .build()
        .into()
}

fn get(port: u16, path: &str, timeout: Duration) -> Result<String, String> {
    let url = format!("{}/_ferrocloud/{path}", endpoint(port));
    agent(timeout)
        .get(&url)
        .call()
        .and_then(|mut response| response.body_mut().read_to_string())
        .map_err(|e| format!("The cloud emulator doesn't answer on port {port}: {e}"))
}

/// Whether the emulator answers on `port`.
pub(super) fn healthy(port: u16) -> bool {
    get(port, "health", Duration::from_secs(2)).is_ok()
}

fn parse_info(health: &str, services: &str) -> Result<CloudInfo, String> {
    #[derive(Deserialize)]
    struct Health {
        version: String,
    }
    #[derive(Deserialize)]
    struct Services {
        services: Vec<CloudService>,
    }
    let unreadable =
        |e: serde_json::Error| format!("The cloud emulator's answer can't be read: {e}");
    let health: Health = serde_json::from_str(health).map_err(unreadable)?;
    let services: Services = serde_json::from_str(services).map_err(unreadable)?;
    Ok(CloudInfo {
        version: health.version,
        services: services.services,
    })
}

/// What the emulator on `port` is and has. Blocks: call it off the UI.
pub fn info(port: u16) -> Result<CloudInfo, String> {
    let timeout = Duration::from_secs(5);
    parse_info(
        &get(port, "health", timeout)?,
        &get(port, "services", timeout)?,
    )
}

/// Empties the emulator on `port`: everything created in it goes.
pub fn reset(port: u16) -> Result<(), String> {
    let url = format!("{}/_ferrocloud/reset", endpoint(port));
    agent(Duration::from_secs(30))
        .post(&url)
        .send_empty()
        .map(drop)
        .map_err(|e| format!("Couldn't reset the cloud emulator: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_what_the_emulator_says() {
        let info = parse_info(
            r#"{"status":"running","version":"1.40.0","services":["sts"]}"#,
            r#"{"services":[{"name":"sts","title":"STS","operations":["GetCallerIdentity","AssumeRole"]}]}"#,
        )
        .unwrap();
        assert_eq!(info.version, "1.40.0");
        assert_eq!(info.services[0].title, "STS");
        assert_eq!(info.services[0].operations.len(), 2);
        assert!(parse_info("<html>", "{}").is_err());
    }

    #[test]
    fn says_when_nothing_answers() {
        // Port 1 is never the emulator's.
        assert!(!healthy(1));
        assert!(info(1).unwrap_err().contains("doesn't answer on port 1"));
    }
}
