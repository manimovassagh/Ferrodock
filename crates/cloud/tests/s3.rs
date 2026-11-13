//! S3 over real connections: what clients send, byte for byte, and what
//! comes back. The AWS CLI script covers the everyday commands; this
//! covers the wire.

use ferrodock_cloud::server::{App, serve};
use ferrodock_cloud::service::Registry;
use ferrodock_cloud::store::Store;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

async fn start() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let app = App::new(Registry::standard(), Store::open(None).unwrap()).unwrap();
    tokio::spawn(serve(listener, app, std::future::pending()));
    address
}

struct Answer {
    status: u16,
    /// Lowercased.
    head: String,
    body: String,
}

impl Answer {
    fn header(&self, name: &str) -> Option<&str> {
        self.head
            .lines()
            .find_map(|line| line.strip_prefix(&format!("{name}: ")))
    }
}

const SIGNED: (&str, &str) = (
    "authorization",
    "AWS4-HMAC-SHA256 Credential=test/20261112/us-east-1/s3/aws4_request, SignedHeaders=host, Signature=0",
);

/// One request on its own connection. `host` is what the Host header
/// says, when it isn't the address.
async fn send(
    address: &str,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &[u8],
) -> Answer {
    let mut stream = TcpStream::connect(address).await.unwrap();
    let mut request = format!(
        "{method} {path} HTTP/1.1\r\nconnection: close\r\ncontent-length: {}\r\n",
        body.len()
    );
    if !headers.iter().any(|(name, _)| *name == "host") {
        request.push_str(&format!("host: {address}\r\n"));
    }
    for (name, value) in headers {
        request.push_str(&format!("{name}: {value}\r\n"));
    }
    request.push_str("\r\n");
    stream.write_all(request.as_bytes()).await.unwrap();
    stream.write_all(body).await.unwrap();
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

#[tokio::test]
async fn puts_and_gets_an_object() {
    let address = start().await;
    assert_eq!(
        send(&address, "PUT", "/photos", &[SIGNED], b"")
            .await
            .status,
        200
    );
    let put = send(
        &address,
        "PUT",
        "/photos/notes/hello%20world.txt",
        &[
            SIGNED,
            ("content-type", "text/plain"),
            ("x-amz-meta-owner", "me"),
        ],
        b"hello world",
    )
    .await;
    assert_eq!(put.status, 200);
    assert_eq!(
        put.header("etag"),
        Some("\"5eb63bbbe01eeed093cb22bb8f5acdc3\"")
    );

    let got = send(
        &address,
        "GET",
        "/photos/notes/hello%20world.txt",
        &[SIGNED],
        b"",
    )
    .await;
    assert_eq!((got.status, got.body.as_str()), (200, "hello world"));
    assert_eq!(got.header("content-type"), Some("text/plain"));
    assert_eq!(got.header("content-length"), Some("11"));
    assert_eq!(got.header("x-amz-meta-owner"), Some("me"));
    assert_eq!(got.header("accept-ranges"), Some("bytes"));

    // A HEAD says how long it is without sending it.
    let head = send(
        &address,
        "HEAD",
        "/photos/notes/hello%20world.txt",
        &[SIGNED],
        b"",
    )
    .await;
    assert_eq!((head.status, head.body.as_str()), (200, ""));
    assert_eq!(head.header("content-length"), Some("11"));

    let part = send(
        &address,
        "GET",
        "/photos/notes/hello%20world.txt",
        &[SIGNED, ("range", "bytes=6-")],
        b"",
    )
    .await;
    assert_eq!((part.status, part.body.as_str()), (206, "world"));
    assert_eq!(part.header("content-range"), Some("bytes 6-10/11"));
    let past = send(
        &address,
        "GET",
        "/photos/notes/hello%20world.txt",
        &[SIGNED, ("range", "bytes=50-")],
        b"",
    )
    .await;
    assert_eq!(past.status, 416);

    let missing = send(&address, "GET", "/photos/nope", &[SIGNED], b"").await;
    assert_eq!(missing.status, 404);
    assert!(
        missing.body.contains("<Code>NoSuchKey</Code>"),
        "{}",
        missing.body
    );
    let nowhere = send(&address, "PUT", "/no-bucket/key", &[SIGNED], b"x").await;
    assert!(
        nowhere.body.contains("<Code>NoSuchBucket</Code>"),
        "{}",
        nowhere.body
    );
}

#[tokio::test]
async fn takes_the_framing_off_a_signed_upload() {
    let address = start().await;
    send(&address, "PUT", "/photos", &[SIGNED], b"").await;
    let framed = b"5;chunk-signature=abc\r\nhello\r\n6;chunk-signature=def\r\n world\r\n0;chunk-signature=fin\r\nx-amz-checksum-crc32:AAAAAA==\r\n\r\n";
    let put = send(
        &address,
        "PUT",
        "/photos/framed.txt",
        &[
            SIGNED,
            ("content-encoding", "aws-chunked"),
            ("x-amz-content-sha256", "STREAMING-UNSIGNED-PAYLOAD-TRAILER"),
            ("x-amz-decoded-content-length", "11"),
        ],
        framed,
    )
    .await;
    assert_eq!(put.status, 200, "{}", put.body);
    let got = send(&address, "GET", "/photos/framed.txt", &[SIGNED], b"").await;
    assert_eq!(got.body, "hello world");
    // How it was sent isn't what it is.
    assert_eq!(got.header("content-encoding"), None);

    let cut = send(
        &address,
        "PUT",
        "/photos/cut.txt",
        &[
            SIGNED,
            ("x-amz-content-sha256", "STREAMING-UNSIGNED-PAYLOAD-TRAILER"),
        ],
        b"5;chunk-signature=abc\r\nhel",
    )
    .await;
    assert_eq!(cut.status, 400);
    assert_eq!(
        send(&address, "GET", "/photos/cut.txt", &[SIGNED], b"")
            .await
            .status,
        404
    );
}

#[tokio::test]
async fn takes_the_bucket_from_the_host_and_unsigned_reads() {
    let address = start().await;
    let port = address.rsplit(':').next().unwrap();
    let host = format!("photos.localhost:{port}");
    let hosted = ("host", host.as_str());
    assert_eq!(
        send(&address, "PUT", "/", &[SIGNED, hosted], b"")
            .await
            .status,
        200
    );
    assert_eq!(
        send(&address, "PUT", "/a.txt", &[SIGNED, hosted], b"by host")
            .await
            .status,
        200
    );
    // The same object, by path, and with no signature at all.
    assert_eq!(
        send(&address, "GET", "/photos/a.txt", &[], b"").await.body,
        "by host"
    );
    let listed = send(&address, "GET", "/?list-type=2", &[hosted], b"").await;
    assert!(listed.body.contains("<Key>a.txt</Key>"), "{}", listed.body);
    let buckets = send(&address, "GET", "/", &[SIGNED], b"").await;
    assert!(
        buckets.body.contains("<Name>photos</Name>"),
        "{}",
        buckets.body
    );
}

#[tokio::test]
async fn answers_only_if_changed() {
    let address = start().await;
    send(&address, "PUT", "/photos", &[SIGNED], b"").await;
    let put = send(&address, "PUT", "/photos/a.txt", &[SIGNED], b"one").await;
    let etag = put.header("etag").unwrap().to_owned();

    let same = send(
        &address,
        "GET",
        "/photos/a.txt",
        &[SIGNED, ("if-none-match", &etag)],
        b"",
    )
    .await;
    assert_eq!((same.status, same.body.as_str()), (304, ""));
    let other = send(
        &address,
        "GET",
        "/photos/a.txt",
        &[SIGNED, ("if-match", "\"nope\"")],
        b"",
    )
    .await;
    assert_eq!(other.status, 412);
    let old = ("if-modified-since", "Thu, 01 Jan 2099 00:00:00 GMT");
    assert_eq!(
        send(&address, "GET", "/photos/a.txt", &[SIGNED, old], b"")
            .await
            .status,
        304
    );

    // Written only if it isn't there yet.
    let again = send(
        &address,
        "PUT",
        "/photos/a.txt",
        &[SIGNED, ("if-none-match", "*")],
        b"two",
    )
    .await;
    assert_eq!(again.status, 412);
    assert_eq!(
        send(&address, "GET", "/photos/a.txt", &[SIGNED], b"")
            .await
            .body,
        "one"
    );
}

#[tokio::test]
async fn builds_an_object_from_parts() {
    let address = start().await;
    send(&address, "PUT", "/photos", &[SIGNED], b"").await;
    let started = send(&address, "POST", "/photos/big?uploads", &[SIGNED], b"").await;
    let id = started
        .body
        .split("<UploadId>")
        .nth(1)
        .unwrap()
        .split('<')
        .next()
        .unwrap();

    let mut etags = Vec::new();
    for (number, bytes) in [(1, &b"hello "[..]), (2, &b"world"[..])] {
        let path = format!("/photos/big?partNumber={number}&uploadId={id}");
        let part = send(&address, "PUT", &path, &[SIGNED], bytes).await;
        assert_eq!(part.status, 200, "{}", part.body);
        etags.push(part.header("etag").unwrap().to_owned());
    }
    let parts = send(
        &address,
        "GET",
        &format!("/photos/big?uploadId={id}"),
        &[SIGNED],
        b"",
    )
    .await;
    assert!(
        parts.body.contains("<PartNumber>2</PartNumber>"),
        "{}",
        parts.body
    );

    let complete = format!(
        "<CompleteMultipartUpload><Part><PartNumber>1</PartNumber><ETag>{}</ETag></Part>\
         <Part><PartNumber>2</PartNumber><ETag>{}</ETag></Part></CompleteMultipartUpload>",
        etags[0], etags[1]
    );
    let done = send(
        &address,
        "POST",
        &format!("/photos/big?uploadId={id}"),
        &[SIGNED],
        complete.as_bytes(),
    )
    .await;
    assert_eq!(done.status, 200, "{}", done.body);
    assert!(done.body.contains("-2&quot;</ETag>"), "{}", done.body);
    assert_eq!(
        send(&address, "GET", "/photos/big", &[SIGNED], b"")
            .await
            .body,
        "hello world"
    );
    // The upload is over.
    let gone = send(
        &address,
        "GET",
        &format!("/photos/big?uploadId={id}"),
        &[SIGNED],
        b"",
    )
    .await;
    assert!(
        gone.body.contains("<Code>NoSuchUpload</Code>"),
        "{}",
        gone.body
    );
}

#[tokio::test]
async fn logs_uploads_without_keeping_them() {
    let address = start().await;
    send(&address, "PUT", "/photos", &[SIGNED], b"").await;
    send(&address, "PUT", "/photos/a.txt", &[SIGNED], b"secret bytes").await;
    send(&address, "GET", "/photos/a.txt", &[SIGNED], b"").await;

    let list = send(&address, "GET", "/_ferrocloud/requests", &[], b"").await;
    let list: serde_json::Value = serde_json::from_str(&list.body).unwrap();
    let requests = list["requests"].as_array().unwrap();
    let operations: Vec<&str> = requests
        .iter()
        .filter_map(|r| r["operation"].as_str())
        .collect();
    assert_eq!(operations, ["CreateBucket", "PutObject", "GetObject"]);

    let put = send(&address, "GET", "/_ferrocloud/requests/2", &[], b"").await;
    assert!(!put.body.contains("secret bytes"));
    let put: serde_json::Value = serde_json::from_str(&put.body).unwrap();
    assert_eq!(put["detail"]["request_body"]["streamed"], true);
    assert_eq!(put["detail"]["request_body"]["size"], 12);
    let got = send(&address, "GET", "/_ferrocloud/requests/3", &[], b"").await;
    assert!(!got.body.contains("secret bytes"));
}
