//! Ferrodock's own API under `/_ferrocloud/`: whether the emulator is up,
//! what it supports, a reset, and the live feed of requests. The app's
//! Cloud page reads it; it isn't part of any cloud's API.

use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use hyper::{Method, StatusCode};
use serde_json::json;
use tokio::sync::{broadcast, mpsc};

use crate::route::parse_pairs;
use crate::server::{App, Body, Response};

/// The paths this API lives under.
pub const PREFIX: &str = "/_ferrocloud/";

/// How often an idle feed says it's still there, so a closed listener is
/// noticed.
const KEEP_ALIVE: Duration = Duration::from_secs(15);

pub fn handle(app: &Arc<App>, method: &Method, path: &str, query: Option<&str>) -> Response {
    match (method, path.strip_prefix(PREFIX).unwrap_or_default()) {
        (&Method::GET, "health") => json_reply(
            StatusCode::OK,
            &json!({
                "status": "running",
                "version": env!("CARGO_PKG_VERSION"),
                "services": app.registry.services().iter().map(|s| s.name()).collect::<Vec<_>>(),
            }),
        ),
        (&Method::GET, "services") => json_reply(
            StatusCode::OK,
            &json!({
                "services": app.registry.services().iter().map(|s| json!({
                    "name": s.name(),
                    "title": s.title(),
                    "operations": s.operations(),
                })).collect::<Vec<_>>(),
            }),
        ),
        (&Method::POST, "reset") => {
            let done = app
                .store
                .reset()
                .and_then(|()| app.registry.init(&app.store));
            app.events.clear();
            match done {
                Ok(()) => json_reply(StatusCode::OK, &json!({ "reset": true })),
                Err(e) => json_reply(StatusCode::INTERNAL_SERVER_ERROR, &json!({ "error": e })),
            }
        }
        (&Method::GET, "events") => {
            let after = parse_pairs(query.unwrap_or_default())
                .iter()
                .find(|(k, _)| k == "after")
                .and_then(|(_, v)| v.parse().ok())
                .unwrap_or(0);
            events(app, after)
        }
        _ => json_reply(
            StatusCode::NOT_FOUND,
            &json!({ "error": format!("no {method} {path} here") }),
        ),
    }
}

fn json_reply(status: StatusCode, value: &serde_json::Value) -> Response {
    hyper::Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Body::full(Bytes::from(value.to_string())))
        .expect("a valid response")
}

/// Requests as server-sent events: those remembered after `after`, then
/// each new one as it finishes.
fn events(app: &Arc<App>, after: u64) -> Response {
    let (past, mut feed) = app.events.follow(after);
    let (tx, rx) = mpsc::channel::<Bytes>(64);
    tokio::spawn(async move {
        let line = |event: &crate::events::Event| {
            Bytes::from(format!(
                "id: {}\ndata: {}\n\n",
                event.id,
                serde_json::to_string(event).unwrap_or_default()
            ))
        };
        for event in &past {
            if tx.send(line(event)).await.is_err() {
                return;
            }
        }
        loop {
            let next = tokio::time::timeout(KEEP_ALIVE, feed.recv()).await;
            let chunk = match next {
                Ok(Ok(event)) => line(&event),
                // Too many at once for this listener: it asks again with
                // the last ID it saw.
                Ok(Err(broadcast::error::RecvError::Lagged(_))) => continue,
                Ok(Err(broadcast::error::RecvError::Closed)) => return,
                Err(_) => Bytes::from_static(b": still here\n\n"),
            };
            if tx.send(chunk).await.is_err() {
                return;
            }
        }
    });
    hyper::Response::builder()
        .header("content-type", "text/event-stream")
        .header("cache-control", "no-cache")
        .body(Body::stream(rx))
        .expect("a valid response")
}
