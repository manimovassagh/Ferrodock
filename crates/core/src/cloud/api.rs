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

/// A request the emulator answered, as its log lists it.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(default)]
pub struct CloudRequest {
    /// Counts up from 1 while the emulator runs.
    pub id: u64,
    /// When it arrived, in milliseconds since 1970.
    pub time: u64,
    pub service: String,
    pub operation: String,
    /// What it was about, e.g. a role's name; empty when unknown.
    pub resource: String,
    pub method: String,
    pub path: String,
    pub status: u16,
    /// The AWS error code, for a request that failed.
    pub error: String,
    /// What the error said.
    pub message: String,
    pub duration_ms: u64,
    pub region: String,
    /// What made the request: the CLI, an SDK, Terraform.
    pub user_agent: String,
    pub request_id: String,
}

impl CloudRequest {
    pub fn failed(&self) -> bool {
        self.status >= 400
    }

    /// Who sent it, in a word or two: `aws-cli/2.31.0` from a long user
    /// agent.
    pub fn client(&self) -> &str {
        self.user_agent
            .split_whitespace()
            .next()
            .unwrap_or_default()
    }
}

/// A body as the emulator's log keeps it.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(default)]
pub struct CloudBody {
    /// Its text, as far as it's kept; secrets are masked.
    pub text: String,
    /// How long the whole body was, in bytes.
    pub size: u64,
    pub truncated: bool,
    /// Not text, so only its size is known.
    pub binary: bool,
}

/// The headers and bodies of one request and its reply.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(default)]
pub struct CloudDetail {
    pub request_headers: Vec<(String, String)>,
    pub request_body: CloudBody,
    pub response_headers: Vec<(String, String)>,
    pub response_body: CloudBody,
}

/// The requests after `after`, oldest first, and the newest ID there is.
/// A newest ID below `after` means the emulator started again.
pub fn requests(port: u16, after: u64) -> Result<(Vec<CloudRequest>, u64), String> {
    #[derive(Deserialize)]
    struct Listed {
        #[serde(default)]
        last: u64,
        requests: Vec<CloudRequest>,
    }
    let text = get(
        port,
        &format!("requests?after={after}"),
        Duration::from_secs(5),
    )?;
    let listed: Listed = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    Ok((listed.requests, listed.last))
}

/// The headers and bodies of a request the emulator still remembers.
pub fn request_detail(port: u16, id: u64) -> Result<CloudDetail, String> {
    #[derive(Deserialize)]
    struct Found {
        detail: CloudDetail,
    }
    let text = get(port, &format!("requests/{id}"), Duration::from_secs(5))
        .map_err(|_| "The emulator no longer remembers this request.".to_owned())?;
    serde_json::from_str::<Found>(&text)
        .map(|found| found.detail)
        .map_err(|e| e.to_string())
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
    fn reads_a_logged_request() {
        let request: CloudRequest = serde_json::from_str(
            r#"{"id":3,"time":1,"service":"iam","operation":"GetRole","resource":"app","method":"POST",
                "path":"/","status":404,"error":"NoSuchEntity","message":"gone","duration_ms":2,
                "region":"us-east-1","user_agent":"aws-cli/2.31.0 md/awscrt#0.27","request_id":"r","new":1}"#,
        )
        .unwrap();
        assert!(request.failed());
        assert_eq!(request.client(), "aws-cli/2.31.0");
        assert_eq!(
            (request.operation.as_str(), request.resource.as_str()),
            ("GetRole", "app")
        );
    }

    #[test]
    fn says_when_nothing_answers() {
        // Port 1 is never the emulator's.
        assert!(!healthy(1));
        assert!(info(1).unwrap_err().contains("doesn't answer on port 1"));
    }
}
