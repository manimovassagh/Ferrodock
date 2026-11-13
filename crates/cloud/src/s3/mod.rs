//! S3: buckets and objects over the object store in [`crate::objects`].
//!
//! Both ways of naming a bucket work: in the path
//! (`localhost:4566/photos/a.jpg`) and in the host
//! (`photos.localhost:4566/a.jpg`). Uploads are written to disk as they
//! arrive and downloads read from it, so an object's size isn't bounded
//! by memory. Nothing is checked against policies or signatures.
//!
//! - `buckets.rs`: buckets, versioning, and the documents kept with a
//!   bucket (CORS, policy, lifecycle…), stored and given back.
//! - `objects.rs`: putting, getting, copying, deleting and tagging.
//! - `list.rs`: listing buckets, objects and versions.
//! - `multipart.rs`: uploads in parts.
//! - `chunked.rs`: the framing the CLI and SDKs put around an upload.
//! - `xml.rs`: reading request documents and writing replies.

mod buckets;
mod chunked;
mod list;
mod multipart;
mod objects;
mod xml;

use bytes::Bytes;
use http_body_util::BodyExt;
use hyper::{HeaderMap, Method};

use crate::objects::{Meta, Object, Problem, Writer, Written, records};
use crate::protocol::{Fault, Reply};
use crate::service::{Call, Handled, Service, Upload};
use crate::store::Store;

/// The most a request document may be (a list of keys to delete, a
/// bucket's policy): objects themselves aren't bounded.
const MAX_DOCUMENT: usize = 8 * 1024 * 1024;

pub struct S3;

impl Service for S3 {
    fn name(&self) -> &'static str {
        "s3"
    }

    fn title(&self) -> &'static str {
        "S3"
    }

    fn signing_names(&self) -> &'static [&'static str] {
        &["s3"]
    }

    fn operations(&self) -> &'static [&'static str] {
        OPERATIONS
    }

    fn init(&self, store: &Store) -> Result<(), String> {
        records::init(store)
    }

    /// A public read, or a presigned URL opened in a browser, names no
    /// service: those are S3's.
    fn takes_unsigned(&self) -> bool {
        true
    }

    /// Every PUT: most are objects' bytes, and the few that aren't (a
    /// bucket's settings) are small and read whole by the handler.
    fn streams(&self, method: &Method, _headers: &HeaderMap) -> bool {
        method == Method::PUT
    }

    fn handle<'a>(&'a self, call: Call, store: &'a Store) -> Handled<'a> {
        Box::pin(async move { dispatch(store, call, None).await })
    }

    fn handle_stream<'a>(&'a self, call: Call, upload: Upload, store: &'a Store) -> Handled<'a> {
        Box::pin(async move { dispatch(store, call, Some(upload)).await })
    }
}

const OPERATIONS: &[&str] = &[
    "AbortMultipartUpload",
    "CompleteMultipartUpload",
    "CopyObject",
    "CreateBucket",
    "CreateMultipartUpload",
    "DeleteBucket",
    "DeleteBucketCors",
    "DeleteBucketEncryption",
    "DeleteBucketLifecycle",
    "DeleteBucketOwnershipControls",
    "DeleteBucketPolicy",
    "DeleteBucketReplication",
    "DeleteBucketTagging",
    "DeleteBucketWebsite",
    "DeleteObject",
    "DeleteObjectTagging",
    "DeleteObjects",
    "DeletePublicAccessBlock",
    "GetBucketAccelerateConfiguration",
    "GetBucketAcl",
    "GetBucketCors",
    "GetBucketEncryption",
    "GetBucketLifecycleConfiguration",
    "GetBucketLocation",
    "GetBucketLogging",
    "GetBucketNotificationConfiguration",
    "GetBucketOwnershipControls",
    "GetBucketPolicy",
    "GetBucketReplication",
    "GetBucketRequestPayment",
    "GetBucketTagging",
    "GetBucketVersioning",
    "GetBucketWebsite",
    "GetObject",
    "GetObjectAcl",
    "GetObjectLockConfiguration",
    "GetObjectTagging",
    "GetPublicAccessBlock",
    "HeadBucket",
    "HeadObject",
    "ListBuckets",
    "ListMultipartUploads",
    "ListObjectVersions",
    "ListObjects",
    "ListObjectsV2",
    "ListParts",
    "PutBucketAccelerateConfiguration",
    "PutBucketAcl",
    "PutBucketCors",
    "PutBucketEncryption",
    "PutBucketLifecycleConfiguration",
    "PutBucketLogging",
    "PutBucketNotificationConfiguration",
    "PutBucketOwnershipControls",
    "PutBucketPolicy",
    "PutBucketReplication",
    "PutBucketRequestPayment",
    "PutBucketTagging",
    "PutBucketVersioning",
    "PutBucketWebsite",
    "PutObject",
    "PutObjectAcl",
    "PutObjectLockConfiguration",
    "PutObjectTagging",
    "PutPublicAccessBlock",
    "UploadPart",
    "UploadPartCopy",
];

/// A request with what it's aimed at worked out.
struct Request<'a> {
    store: &'a Store,
    call: Call,
    bucket: String,
    /// Empty for a request about the bucket itself.
    key: String,
}

impl Request<'_> {
    /// A query parameter's value.
    fn query(&self, name: &str) -> Option<&str> {
        self.call
            .query
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    /// Whether a query parameter is there, with or without a value, as
    /// `?versioning` is.
    fn has(&self, name: &str) -> bool {
        self.call.query.iter().any(|(k, _)| k == name)
    }

    fn header(&self, name: &str) -> Option<&str> {
        self.call.headers.get(name).and_then(|v| v.to_str().ok())
    }

    /// The request's document, as text.
    fn document(&self) -> String {
        String::from_utf8_lossy(&self.call.body).into_owned()
    }
}

/// The bucket a request names in its host, if it does:
/// `photos.localhost:4566`, `photos.s3.localhost`,
/// `photos.s3.eu-west-1.amazonaws.com`.
fn bucket_in_host(host: &str) -> Option<&str> {
    let host = host.rsplit_once(':').map_or(host, |(name, port)| {
        if port.bytes().all(|b| b.is_ascii_digit()) {
            name
        } else {
            host
        }
    });
    let before = host
        .strip_suffix(".localhost")
        .or_else(|| host.strip_suffix(".amazonaws.com"))?;
    // Past the bucket there may be `s3`, and a region.
    let bucket = match before.find(".s3") {
        Some(at) if before[at + 3..].is_empty() || before[at + 3..].starts_with(['.', '-']) => {
            &before[..at]
        }
        _ if before == "s3" || before.starts_with("s3.") || before.starts_with("s3-") => "",
        _ => before,
    };
    (!bucket.is_empty()).then_some(bucket)
}

/// A path as sent, with `%2F` and the like undone. A `+` is a plus here,
/// unlike in a query.
fn decode_path(path: &str) -> String {
    let bytes = path.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = bytes
            .get(i + 1..i + 3)
            .filter(|_| bytes[i] == b'%')
            .and_then(|h| std::str::from_utf8(h).ok())
            .and_then(|h| u8::from_str_radix(h, 16).ok());
        match hex {
            Some(byte) => {
                out.push(byte);
                i += 3;
            }
            None => {
                out.push(bytes[i]);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Text made safe in a URL, keeping `/`: how keys are listed when the
/// caller asks for `encoding-type=url`.
fn url_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                out.push(byte as char)
            }
            byte => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// The bucket and key a request is aimed at; both empty for the account.
fn target(headers: &HeaderMap, path: &str) -> (String, String) {
    let path = decode_path(path.strip_prefix('/').unwrap_or(path));
    let host = headers
        .get("host")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    match bucket_in_host(host) {
        Some(bucket) => (bucket.to_owned(), path),
        None => match path.split_once('/') {
            Some((bucket, key)) => (bucket.to_owned(), key.to_owned()),
            None => (path, String::new()),
        },
    }
}

/// An error as S3 words it.
fn fault(problem: Problem) -> Fault {
    match problem {
        Problem::NoBucket(_) => {
            Fault::new(404, "NoSuchBucket", "The specified bucket does not exist")
        }
        Problem::BucketExists(_) => Fault::new(
            409,
            "BucketAlreadyOwnedByYou",
            "Your previous request to create the named bucket succeeded and you already own it.",
        ),
        Problem::BucketNotEmpty(_) => Fault::new(
            409,
            "BucketNotEmpty",
            "The bucket you tried to delete is not empty",
        ),
        Problem::NoObject { .. } => {
            Fault::new(404, "NoSuchKey", "The specified key does not exist.")
        }
        Problem::NoVersion { .. } => Fault::new(
            404,
            "NoSuchVersion",
            "The specified version does not exist.",
        ),
        Problem::NoUpload(_) => Fault::new(
            404,
            "NoSuchUpload",
            "The specified upload does not exist. The upload ID may be invalid, or the upload may have been \
             aborted or completed.",
        ),
        Problem::BadPart(why) => Fault::new(
            400,
            "InvalidPart",
            format!("One or more of the specified parts could not be found: {why}."),
        ),
        Problem::Internal(why) => Fault::new(500, "InternalError", why),
    }
}

/// A 200 with an XML document.
fn xml_reply(root: &str, inner: &str) -> Reply {
    Reply::new(200, "application/xml", xml::document(root, inner))
}

/// An ETag as headers and documents carry it: in quotes.
fn quoted(etag: &str) -> String {
    format!("\"{etag}\"")
}

/// A version ID to give back: none for a bucket that never kept versions.
fn version_header(reply: Reply, object: &Object) -> Reply {
    if object.version == records::NO_VERSION {
        reply
    } else {
        reply.header("x-amz-version-id", &object.version)
    }
}

/// The headers a write may set on an object, kept and given back.
const KEPT_HEADERS: &[&str] = &[
    "cache-control",
    "content-disposition",
    "content-encoding",
    "content-language",
    "expires",
    "x-amz-website-redirect-location",
];

/// `a=1&b=2` as the `x-amz-tagging` header carries tags.
fn tags_in_header(value: &str) -> std::collections::BTreeMap<String, String> {
    crate::route::parse_pairs(value).into_iter().collect()
}

/// What's kept with an object, from the headers of the request that
/// writes it.
fn meta(headers: &HeaderMap) -> Meta {
    let mut meta = Meta::default();
    for (name, value) in headers {
        let (name, Ok(value)) = (name.as_str(), value.to_str()) else {
            continue;
        };
        if name == "content-type" {
            meta.content_type = value.to_owned();
        } else if name == "content-encoding" {
            // `aws-chunked` is how it was sent, not what it is.
            let kept: Vec<&str> = value
                .split(',')
                .map(str::trim)
                .filter(|v| !v.is_empty() && *v != "aws-chunked")
                .collect();
            if !kept.is_empty() {
                meta.headers.insert(name.to_owned(), kept.join(", "));
            }
        } else if KEPT_HEADERS.contains(&name) {
            meta.headers.insert(name.to_owned(), value.to_owned());
        } else if let Some(key) = name.strip_prefix("x-amz-meta-") {
            meta.user.insert(key.to_owned(), value.to_owned());
        } else if name == "x-amz-tagging" {
            meta.tags = tags_in_header(value);
        }
    }
    meta
}

/// Whether a body comes in `aws-chunked` framing.
fn is_chunked(headers: &HeaderMap) -> bool {
    let header = |name: &str| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
    };
    header("x-amz-content-sha256").starts_with("STREAMING-")
        || header("content-encoding").contains("aws-chunked")
}

/// An upload being read: the object's bytes, piece by piece, with any
/// framing taken off.
struct Incoming {
    upload: Upload,
    decoder: Option<chunked::Decoder>,
}

impl Incoming {
    fn new(upload: Upload, headers: &HeaderMap) -> Self {
        Self {
            upload,
            decoder: is_chunked(headers).then(chunked::Decoder::default),
        }
    }

    /// The next piece; None at the end. A body that stops short of what
    /// it promised is an error.
    async fn next(&mut self) -> Result<Option<Bytes>, Fault> {
        let cut_short = || {
            Fault::bad_request(
                "IncompleteBody",
                "The request body ended before it was complete.",
            )
        };
        loop {
            let Some(frame) = self.upload.frame().await else {
                if self.decoder.as_ref().is_some_and(|d| !d.complete()) {
                    return Err(cut_short());
                }
                return Ok(None);
            };
            let Ok(data) = frame.map_err(|_| cut_short())?.into_data() else {
                continue;
            };
            match &mut self.decoder {
                Some(decoder) => {
                    let mut plain = Vec::with_capacity(data.len());
                    decoder
                        .feed(&data, &mut plain)
                        .map_err(|why| Fault::bad_request("InvalidChunk", why))?;
                    if !plain.is_empty() {
                        return Ok(Some(Bytes::from(plain)));
                    }
                }
                None => return Ok(Some(data)),
            }
        }
    }
}

/// Writes an upload to a file as it arrives.
async fn save(store: &Store, upload: Upload, headers: &HeaderMap) -> Result<Written, Fault> {
    let internal = |why: String| Fault::new(500, "InternalError", why);
    let mut incoming = Incoming::new(upload, headers);
    let mut writer = Writer::create(store).await.map_err(internal)?;
    while let Some(bytes) = incoming.next().await? {
        writer.write(&bytes).await.map_err(internal)?;
    }
    writer.finish().await.map_err(internal)
}

/// Reads a small request document whole.
async fn document(upload: Upload, headers: &HeaderMap) -> Result<Bytes, Fault> {
    let mut incoming = Incoming::new(upload, headers);
    let mut whole = Vec::new();
    while let Some(bytes) = incoming.next().await? {
        if whole.len() + bytes.len() > MAX_DOCUMENT {
            return Err(Fault::new(
                400,
                "MaxMessageLengthExceeded",
                "Your request was too big.",
            ));
        }
        whole.extend_from_slice(&bytes);
    }
    Ok(Bytes::from(whole))
}

async fn dispatch(store: &Store, mut call: Call, upload: Option<Upload>) -> Result<Reply, Fault> {
    let (bucket, key) = target(&call.headers, &call.path);
    let has = |name: &str| call.query.iter().any(|(k, _)| k == name);
    // An upload that's an object's bytes goes straight to disk; anything
    // else sent with PUT is a document, read whole.
    let upload = match upload {
        Some(upload) => {
            let bytes = !key.is_empty()
                && !call.headers.contains_key("x-amz-copy-source")
                && !["tagging", "acl", "retention", "legal-hold"]
                    .into_iter()
                    .any(has);
            if bytes {
                Some(upload)
            } else {
                call.body = document(upload, &call.headers).await?;
                None
            }
        }
        None => None,
    };
    let method = call.method.clone();
    let request = Request {
        store,
        call,
        bucket,
        key,
    };
    if request.bucket.is_empty() {
        return match method {
            Method::GET | Method::HEAD => list::buckets(&request),
            _ => Err(method_not_allowed()),
        };
    }
    if request.key.is_empty() {
        return buckets::handle(&request, &method).await;
    }
    match upload {
        Some(upload) if request.has("partNumber") => multipart::upload_part(&request, upload).await,
        Some(upload) => objects::put(&request, upload).await,
        None => objects::handle(&request, &method).await,
    }
}

fn method_not_allowed() -> Fault {
    Fault::new(
        405,
        "MethodNotAllowed",
        "The specified method is not allowed against this resource.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_bucket_in_the_host() {
        for (host, bucket) in [
            ("photos.localhost:4566", Some("photos")),
            ("photos.localhost", Some("photos")),
            ("my.dotted.bucket.localhost:4566", Some("my.dotted.bucket")),
            ("photos.s3.localhost:4566", Some("photos")),
            ("photos.s3.eu-west-1.amazonaws.com", Some("photos")),
            ("photos.s3.amazonaws.com", Some("photos")),
            ("photos.s3-eu-west-1.amazonaws.com", Some("photos")),
            ("s3.localhost:4566", None),
            ("s3.eu-west-1.amazonaws.com", None),
            ("localhost:4566", None),
            ("127.0.0.1:4566", None),
            ("ferrocloud:4566", None),
            ("host.docker.internal:4566", None),
        ] {
            assert_eq!(bucket_in_host(host), bucket, "{host}");
        }
    }

    #[test]
    fn finds_the_bucket_and_key() {
        let headers = |host: &'static str| {
            let mut map = HeaderMap::new();
            map.insert("host", host.parse().unwrap());
            map
        };
        let at = |host, path| target(&headers(host), path);
        assert_eq!(at("localhost:4566", "/"), (String::new(), String::new()));
        assert_eq!(
            at("localhost:4566", "/photos"),
            ("photos".to_owned(), String::new())
        );
        assert_eq!(
            at("localhost:4566", "/photos/"),
            ("photos".to_owned(), String::new())
        );
        assert_eq!(
            at("localhost:4566", "/photos/2026/a%20b+c.jpg"),
            ("photos".to_owned(), "2026/a b+c.jpg".to_owned())
        );
        assert_eq!(
            at("photos.localhost:4566", "/2026/a.jpg"),
            ("photos".to_owned(), "2026/a.jpg".to_owned())
        );
        assert_eq!(
            at("photos.localhost:4566", "/"),
            ("photos".to_owned(), String::new())
        );
    }

    #[test]
    fn reads_what_to_keep_with_an_object() {
        let mut headers = HeaderMap::new();
        for (name, value) in [
            ("content-type", "image/png"),
            ("content-encoding", "aws-chunked, gzip"),
            ("cache-control", "max-age=60"),
            ("x-amz-meta-owner", "me"),
            ("x-amz-tagging", "env=dev&team=a%20b"),
            ("authorization", "secret"),
        ] {
            headers.insert(name, value.parse().unwrap());
        }
        let meta = meta(&headers);
        assert_eq!(meta.content_type, "image/png");
        assert_eq!(meta.headers["content-encoding"], "gzip");
        assert_eq!(meta.headers["cache-control"], "max-age=60");
        assert_eq!(meta.headers.len(), 2);
        assert_eq!(meta.user["owner"], "me");
        assert_eq!(meta.tags["team"], "a b");
        assert!(is_chunked(&headers));
        assert_eq!(url_encode("a b/ç+.txt"), "a%20b/%C3%A7%2B.txt");
    }
}
