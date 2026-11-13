//! The server end to end, over real HTTP, with a made-up service.

use std::sync::Arc;
use std::time::Duration;

use ferrodock_cloud::protocol::{Fault, Protocol, Reply};
use ferrodock_cloud::server::{App, serve};
use ferrodock_cloud::service::{Call, Handled, Registry, Service};
use ferrodock_cloud::store::Store;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// A service that says back what it was asked, and keeps a counter in the
/// database.
struct Echo;

impl Service for Echo {
    fn name(&self) -> &'static str {
        "echo"
    }
    fn title(&self) -> &'static str {
        "Echo"
    }
    fn signing_names(&self) -> &'static [&'static str] {
        &["echo"]
    }
    fn target_prefixes(&self) -> &'static [&'static str] {
        &["Echo_20260101"]
    }
    fn operations(&self) -> &'static [&'static str] {
        &["Say", "Count", "Fail"]
    }
    fn init(&self, store: &Store) -> Result<(), String> {
        store
            .db()
            .execute_batch("CREATE TABLE IF NOT EXISTS echo_calls (n INTEGER)")
            .map_err(|e| e.to_string())
    }
    fn handle<'a>(&'a self, call: Call, store: &'a Store) -> Handled<'a> {
        Box::pin(async move {
            match call.route.operation.as_deref() {
                Some("Say") => Ok(Reply::ok(call.route.protocol, call.body)),
                Some("Count") => {
                    let db = store.db();
                    db.execute("INSERT INTO echo_calls VALUES (1)", [])
                        .map_err(|e| Fault::internal(e.to_string()))?;
                    let n: i64 = db
                        .query_row("SELECT COUNT(*) FROM echo_calls", [], |r| r.get(0))
                        .map_err(|e| Fault::internal(e.to_string()))?;
                    Ok(Reply::ok(call.route.protocol, format!("{{\"count\":{n}}}")))
                }
                Some("Fail") => Err(Fault::bad_request("ValidationException", "no good")),
                _ => Ok(Reply::ok(Protocol::RestJson, "{}").operation("Root")),
            }
        })
    }
}

/// Starts a server on a free port and returns its address.
async fn start() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let app = App::new(Registry::default().with(Echo), Store::open(None).unwrap()).unwrap();
    tokio::spawn(serve(listener, app, std::future::pending()));
    address
}

struct Answer {
    status: u16,
    head: String,
    body: String,
}

/// One HTTP/1.1 request on its own connection.
async fn send(
    address: &str,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> Answer {
    let mut stream = TcpStream::connect(address).await.unwrap();
    let mut request = format!(
        "{method} {path} HTTP/1.1\r\nhost: {address}\r\nconnection: close\r\ncontent-length: {}\r\n",
        body.len()
    );
    for (name, value) in headers {
        request.push_str(&format!("{name}: {value}\r\n"));
    }
    request.push_str("\r\n");
    request.push_str(body);
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).await.unwrap();
    let text = String::from_utf8_lossy(&raw).into_owned();
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
    Answer {
        status: head
            .split(' ')
            .nth(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0),
        head: head.to_lowercase(),
        body: body.to_owned(),
    }
}

const JSON: (&str, &str) = ("content-type", "application/x-amz-json-1.1");

fn signed_for(service: &str) -> String {
    format!(
        "AWS4-HMAC-SHA256 Credential=test/20261112/eu-west-1/{service}/aws4_request, SignedHeaders=host, Signature=0"
    )
}

#[tokio::test]
async fn calls_the_service_a_request_is_for() {
    let address = start().await;
    // By its target alone, unsigned.
    let said = send(
        &address,
        "POST",
        "/",
        &[JSON, ("x-amz-target", "Echo_20260101.Say")],
        r#"{"hello":"there"}"#,
    )
    .await;
    assert_eq!(said.status, 200);
    assert_eq!(said.body, r#"{"hello":"there"}"#);
    assert!(
        said.head
            .contains("content-type: application/x-amz-json-1.1")
    );
    assert!(said.head.contains("x-amzn-requestid: "));
    assert!(said.head.contains("server: ferrocloud"));

    // By what it was signed for, with the action in a form.
    let auth = signed_for("echo");
    let form = [
        ("authorization", auth.as_str()),
        ("content-type", "application/x-www-form-urlencoded"),
    ];
    let said = send(&address, "POST", "/", &form, "Action=Say&Version=1").await;
    assert_eq!(said.status, 200);
    assert_eq!(said.body, "Action=Say&Version=1");
}

#[tokio::test]
async fn answers_errors_in_the_callers_protocol() {
    let address = start().await;
    let failed = send(
        &address,
        "POST",
        "/",
        &[JSON, ("x-amz-target", "Echo_20260101.Fail")],
        "{}",
    )
    .await;
    assert_eq!(failed.status, 400);
    assert_eq!(
        failed.body,
        r#"{"__type":"ValidationException","message":"no good"}"#
    );
    assert!(
        failed
            .head
            .contains("x-amzn-errortype: validationexception")
    );

    // A service that isn't there, asked in each protocol.
    let json = send(
        &address,
        "POST",
        "/",
        &[JSON, ("x-amz-target", "Nope_1.Do")],
        "{}",
    )
    .await;
    assert_eq!(json.status, 400);
    assert!(
        json.body
            .contains(r#""__type":"UnknownOperationException""#),
        "{}",
        json.body
    );
    assert!(json.body.contains("the service behind 'Nope_1'"));

    let auth = signed_for("sts");
    let form = [
        ("authorization", auth.as_str()),
        ("content-type", "application/x-www-form-urlencoded"),
    ];
    let query = send(&address, "POST", "/", &form, "Action=GetCallerIdentity").await;
    assert_eq!(query.status, 400);
    assert!(
        query.body.contains("<Code>InvalidAction</Code>"),
        "{}",
        query.body
    );
    assert!(query.body.contains("the service &apos;sts&apos;"));

    let auth = signed_for("s3");
    let rest = send(
        &address,
        "GET",
        "/a-bucket",
        &[("authorization", auth.as_str())],
        "",
    )
    .await;
    assert_eq!(rest.status, 501);
    assert!(
        rest.body.contains("<Error><Code>NotImplemented</Code>"),
        "{}",
        rest.body
    );
}

#[tokio::test]
async fn admin_lists_services_and_resets() {
    let address = start().await;
    let health = send(&address, "GET", "/_ferrocloud/health", &[], "").await;
    assert_eq!(health.status, 200);
    let health: serde_json::Value = serde_json::from_str(&health.body).unwrap();
    assert_eq!(health["status"], "running");
    assert_eq!(health["services"], serde_json::json!(["echo"]));

    let services = send(&address, "GET", "/_ferrocloud/services", &[], "").await;
    let services: serde_json::Value = serde_json::from_str(&services.body).unwrap();
    assert_eq!(services["services"][0]["title"], "Echo");
    assert_eq!(
        services["services"][0]["operations"],
        serde_json::json!(["Say", "Count", "Fail"])
    );

    let count = [JSON, ("x-amz-target", "Echo_20260101.Count")];
    send(&address, "POST", "/", &count, "{}").await;
    assert_eq!(
        send(&address, "POST", "/", &count, "{}").await.body,
        r#"{"count":2}"#
    );
    // A reset empties the data, and the service's table is there again.
    let reset = send(&address, "POST", "/_ferrocloud/reset", &[], "").await;
    assert_eq!(reset.body, r#"{"reset":true}"#);
    assert_eq!(
        send(&address, "POST", "/", &count, "{}").await.body,
        r#"{"count":1}"#
    );

    assert_eq!(
        send(&address, "GET", "/_ferrocloud/nope", &[], "")
            .await
            .status,
        404
    );
    assert_eq!(
        send(&address, "GET", "/_ferrocloud/reset", &[], "")
            .await
            .status,
        404
    );
}

#[tokio::test]
async fn streams_the_requests_it_served() {
    let address = start().await;
    let auth = signed_for("echo");
    send(
        &address,
        "POST",
        "/",
        &[
            JSON,
            ("x-amz-target", "Echo_20260101.Say"),
            ("user-agent", "aws-cli/2.0"),
            ("authorization", auth.as_str()),
        ],
        "{}",
    )
    .await;
    send(
        &address,
        "POST",
        "/",
        &[JSON, ("x-amz-target", "Echo_20260101.Fail")],
        "{}",
    )
    .await;
    send(
        &address,
        "GET",
        "/thing",
        &[("authorization", auth.as_str())],
        "",
    )
    .await;

    // The feed never ends: read what's there, then stop.
    let mut stream = TcpStream::connect(&address).await.unwrap();
    stream
        .write_all(
            format!("GET /_ferrocloud/events?after=1 HTTP/1.1\r\nhost: {address}\r\n\r\n")
                .as_bytes(),
        )
        .await
        .unwrap();
    let mut seen = String::new();
    let mut buffer = [0u8; 4096];
    while seen.matches("data: ").count() < 2 {
        let n = tokio::time::timeout(Duration::from_secs(5), stream.read(&mut buffer))
            .await
            .expect("the feed sends what it has")
            .unwrap();
        assert!(n > 0, "the feed closed early: {seen}");
        seen.push_str(&String::from_utf8_lossy(&buffer[..n]));
    }
    assert!(
        seen.to_lowercase()
            .contains("content-type: text/event-stream")
    );
    let events: Vec<serde_json::Value> = seen
        .lines()
        .filter_map(|line| line.strip_prefix("data: "))
        .map(|json| serde_json::from_str(json).unwrap())
        .collect();
    // The first one was skipped with `after=1`.
    assert_eq!(events[0]["id"], 2);
    assert_eq!(events[0]["operation"], "Fail");
    assert_eq!(events[0]["status"], 400);
    assert_eq!(events[0]["error"], "ValidationException");
    assert_eq!(events[0]["service"], "echo");
    // A REST call's operation comes from the service's reply.
    assert_eq!(events[1]["operation"], "Root");
    assert_eq!(events[1]["method"], "GET");
    assert_eq!(events[1]["path"], "/thing");
    assert_eq!(events[1]["region"], "eu-west-1");

    // And the admin API's own requests aren't in it.
    let _ = Arc::new(());
    assert!(!seen.contains("_ferrocloud"));
}

#[tokio::test]
async fn remembers_what_was_sent_and_answered() {
    let address = start().await;
    let auth = signed_for("echo");
    let said = r#"{"QueueName":"jobs","SecretString":"hunter2"}"#;
    send(
        &address,
        "POST",
        "/",
        &[
            JSON,
            ("x-amz-target", "Echo_20260101.Say"),
            ("authorization", auth.as_str()),
        ],
        said,
    )
    .await;
    send(
        &address,
        "POST",
        "/",
        &[JSON, ("x-amz-target", "Echo_20260101.Fail")],
        "{}",
    )
    .await;

    let list = send(&address, "GET", "/_ferrocloud/requests", &[], "").await;
    let list: serde_json::Value = serde_json::from_str(&list.body).unwrap();
    let requests = list["requests"].as_array().unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0]["operation"], "Say");
    assert_eq!(requests[0]["resource"], "jobs");
    assert_eq!(requests[1]["error"], "ValidationException");
    assert_eq!(requests[1]["message"], "no good");
    // Only what's newer than the one already seen.
    let newer = send(&address, "GET", "/_ferrocloud/requests?after=1", &[], "").await;
    let newer: serde_json::Value = serde_json::from_str(&newer.body).unwrap();
    assert_eq!(newer["requests"].as_array().unwrap().len(), 1);

    let one = send(&address, "GET", "/_ferrocloud/requests/1", &[], "").await;
    let one: serde_json::Value = serde_json::from_str(&one.body).unwrap();
    assert_eq!(one["request"]["id"], 1);
    let detail = &one["detail"];
    // Secrets are masked on the way in and on the way out (Say echoes).
    let masked = r#"{"QueueName":"jobs","SecretString":"***"}"#;
    assert_eq!(detail["request_body"]["text"], masked);
    assert_eq!(detail["response_body"]["text"], masked);
    assert_eq!(detail["request_body"]["size"], said.len());
    let headers = detail["request_headers"].as_array().unwrap();
    let authorization = headers.iter().find(|h| h[0] == "authorization").unwrap();
    assert!(
        authorization[1]
            .as_str()
            .unwrap()
            .ends_with("Signature=***")
    );

    let gone = send(&address, "GET", "/_ferrocloud/requests/99", &[], "").await;
    assert_eq!(gone.status, 404);
}

#[tokio::test]
async fn serves_the_console_and_what_each_service_holds() {
    let address = start().await;
    let health = send(&address, "GET", "/_ferrocloud/health", &[], "").await;
    assert!(health.body.contains(r#""console":"/_ferrocloud/console/""#));

    let page = send(&address, "GET", "/_ferrocloud/console/", &[], "").await;
    assert_eq!(page.status, 200);
    assert!(page.head.contains("content-type: text/html"));
    assert!(page.body.contains("console.js"));
    let moved = send(&address, "GET", "/_ferrocloud/console", &[], "").await;
    assert_eq!(moved.status, 308);
    assert!(moved.head.contains("location: /_ferrocloud/console/"));

    let inventory = send(&address, "GET", "/_ferrocloud/inventory", &[], "").await;
    assert_eq!(inventory.status, 200);
    let inventory: serde_json::Value = serde_json::from_str(&inventory.body).unwrap();
    assert_eq!(inventory["services"][0]["name"], "echo");
    assert_eq!(inventory["services"][0]["operations"], 3);
    assert_eq!(inventory["services"][0]["listings"], serde_json::json!([]));
}
