//! SQS end to end over real HTTP: both protocols, and a receive that
//! waits for a message sent while it waits.

use std::time::{Duration, Instant};

use ferrodock_cloud::server::{App, serve};
use ferrodock_cloud::service::Registry;
use ferrodock_cloud::store::Store;
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

async fn start() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let app = App::new(Registry::standard(), Store::open(None).unwrap()).unwrap();
    tokio::spawn(serve(listener, app, std::future::pending()));
    address
}

/// One request on its own connection; gives the status and body.
async fn post(address: &str, path: &str, headers: &[(&str, &str)], body: &str) -> (u16, String) {
    let mut stream = TcpStream::connect(address).await.unwrap();
    let mut request = format!(
        "POST {path} HTTP/1.1\r\nhost: {address}\r\nconnection: close\r\ncontent-length: {}\r\n",
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
    let status = head
        .split(' ')
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    (status, body.to_owned())
}

/// An operation in the JSON protocol, as current SDKs send it.
async fn json_call(address: &str, operation: &str, input: Value) -> (u16, Value) {
    let target = format!("AmazonSQS.{operation}");
    let (status, body) = post(
        address,
        "/",
        &[
            ("x-amz-target", &target),
            ("content-type", "application/x-amz-json-1.0"),
        ],
        &input.to_string(),
    )
    .await;
    (status, serde_json::from_str(&body).unwrap_or(Value::Null))
}

#[tokio::test]
async fn speaks_both_protocols() {
    let address = start().await;
    let (status, created) = json_call(&address, "CreateQueue", json!({"QueueName": "jobs"})).await;
    assert_eq!(status, 200);
    let url = created["QueueUrl"].as_str().unwrap().to_owned();
    assert!(url.ends_with("/000000000000/jobs"));

    // Sent in the query protocol, to the queue's own URL.
    let (status, sent) = post(
        &address,
        "/000000000000/jobs",
        &[
            ("content-type", "application/x-www-form-urlencoded"),
            (
                "authorization",
                "AWS4-HMAC-SHA256 Credential=k/20261114/us-east-1/sqs/aws4_request, Signature=0",
            ),
        ],
        "Action=SendMessage&MessageBody=hello",
    )
    .await;
    assert_eq!(status, 200);
    assert!(sent.contains("<MD5OfMessageBody>5d41402abc4b2a76b9719d911017c592</MD5OfMessageBody>"));

    // Received in JSON.
    let (_, got) = json_call(&address, "ReceiveMessage", json!({"QueueUrl": url})).await;
    assert_eq!(got["Messages"][0]["Body"], "hello");

    let (status, missing) = json_call(&address, "GetQueueUrl", json!({"QueueName": "nope"})).await;
    assert_eq!(status, 400);
    assert_eq!(missing["__type"], "QueueDoesNotExist");
}

#[tokio::test]
async fn a_waiting_receive_gets_a_message_sent_meanwhile() {
    let address = start().await;
    let (_, created) = json_call(&address, "CreateQueue", json!({"QueueName": "later"})).await;
    let url = created["QueueUrl"].as_str().unwrap().to_owned();

    let sender = {
        let (address, url) = (address.clone(), url.clone());
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(300)).await;
            json_call(
                &address,
                "SendMessage",
                json!({"QueueUrl": url, "MessageBody": "late"}),
            )
            .await
        })
    };
    let started = Instant::now();
    let (_, got) = json_call(
        &address,
        "ReceiveMessage",
        json!({"QueueUrl": url, "WaitTimeSeconds": 5}),
    )
    .await;
    assert_eq!(got["Messages"][0]["Body"], "late");
    // It came as soon as the message did, not at the end of the wait.
    assert!(started.elapsed() < Duration::from_secs(2));
    sender.await.unwrap();

    // With nothing to get, it waits the time asked, then answers empty.
    let started = Instant::now();
    let (_, empty) = json_call(
        &address,
        "ReceiveMessage",
        json!({"QueueUrl": url, "WaitTimeSeconds": 1}),
    )
    .await;
    assert_eq!(empty, json!({}));
    assert!(started.elapsed() >= Duration::from_millis(900));
}
