//! Objects: putting, getting (whole, in part, or only if changed),
//! copying, deleting one or many, and tagging.

use std::collections::BTreeMap;

use hyper::Method;

use super::{
    Request, decode_path, fault, meta, method_not_allowed, multipart, quoted, save, tags_in_header,
    version_header, xml, xml_reply,
};
use crate::clock;
use crate::objects::{Object, Problem, files, records};
use crate::protocol::{Fault, FileBody, Reply};
use crate::service::Upload;

pub(super) async fn handle(request: &Request<'_>, method: &Method) -> Result<Reply, Fault> {
    match *method {
        Method::GET if request.has("uploadId") => multipart::list_parts(request),
        Method::GET if request.has("tagging") => get_tagging(request),
        Method::GET if request.has("acl") => {
            found(request)?;
            Ok(Reply::new(200, "application/xml", super::buckets::acl()).operation("GetObjectAcl"))
        }
        Method::GET => get(request, false),
        Method::HEAD => get(request, true),
        Method::DELETE if request.has("uploadId") => multipart::abort(request),
        Method::DELETE if request.has("tagging") => {
            set_tags(request, BTreeMap::new(), 204, "DeleteObjectTagging")
        }
        Method::DELETE => delete(request),
        Method::POST if request.has("uploads") => multipart::create(request),
        Method::POST if request.has("uploadId") => multipart::complete(request).await,
        // What's sent with PUT and isn't an object's bytes.
        Method::PUT if request.header("x-amz-copy-source").is_some() => {
            if request.has("partNumber") {
                multipart::upload_part_copy(request).await
            } else {
                copy(request).await
            }
        }
        Method::PUT if request.has("tagging") => {
            let document = request.document();
            let tags = xml::blocks(&document, "Tag")
                .into_iter()
                .filter_map(|tag| {
                    Some((
                        xml::text(tag, "Key")?,
                        xml::text(tag, "Value").unwrap_or_default(),
                    ))
                })
                .collect();
            set_tags(request, tags, 200, "PutObjectTagging")
        }
        Method::PUT if request.has("acl") => {
            found(request)?;
            Ok(Reply::empty(200).operation("PutObjectAcl"))
        }
        _ => Err(method_not_allowed()),
    }
}

/// The object a request names: the version it asks for, or the current.
fn found(request: &Request) -> Result<Object, Fault> {
    let version = request.query("versionId").filter(|v| !v.is_empty());
    records::get(request.store, &request.bucket, &request.key, version).map_err(fault)
}

/// The headers every answer about an object carries.
fn describe(mut reply: Reply, object: &Object) -> Reply {
    reply = reply
        .header("etag", &quoted(&object.etag))
        .header("last-modified", &clock::http_date(object.modified / 1000))
        .header("accept-ranges", "bytes")
        .header(
            "content-type",
            if object.meta.content_type.is_empty() {
                "binary/octet-stream"
            } else {
                &object.meta.content_type
            },
        );
    for (name, value) in &object.meta.headers {
        reply = reply.header(name, value);
    }
    for (name, value) in &object.meta.user {
        reply = reply.header(&format!("x-amz-meta-{name}"), value);
    }
    if !object.meta.tags.is_empty() {
        reply = reply.header("x-amz-tagging-count", &object.meta.tags.len().to_string());
    }
    version_header(reply, object)
}

/// Whether an `If-Match`-style header names this ETag: any of a list,
/// quoted or not, or `*`.
fn names_etag(header: &str, etag: &str) -> bool {
    header
        .split(',')
        .map(|one| one.trim().trim_start_matches("W/").trim_matches('"'))
        .any(|one| one == "*" || one == etag)
}

/// The part of an object of `size` bytes a `Range` header asks for, as
/// (start, length). None for the whole; an error when it's past the end.
fn range(header: Option<&str>, size: u64) -> Result<Option<(u64, u64)>, Fault> {
    let Some(spec) = header.and_then(|h| h.trim().strip_prefix("bytes=")) else {
        return Ok(None);
    };
    // Several ranges at once aren't served in parts: the whole is.
    let Some((first, last)) = spec.split_once('-').filter(|_| !spec.contains(',')) else {
        return Ok(None);
    };
    let past = || {
        Fault::new(
            416,
            "InvalidRange",
            "The requested range is not satisfiable",
        )
    };
    let (start, end) = match (first.trim().parse::<u64>(), last.trim().parse::<u64>()) {
        // The last N bytes.
        (Err(_), Ok(n)) if first.trim().is_empty() => {
            if n == 0 || size == 0 {
                return Err(past());
            }
            (size.saturating_sub(n), size - 1)
        }
        (Ok(start), Ok(end)) if start <= end => (start, end.min(size.saturating_sub(1))),
        (Ok(start), Err(_)) if last.trim().is_empty() => (start, size.saturating_sub(1)),
        _ => return Ok(None),
    };
    if start >= size {
        return Err(past());
    }
    Ok(Some((start, end - start + 1)))
}

fn get(request: &Request, head: bool) -> Result<Reply, Fault> {
    let operation = if head { "HeadObject" } else { "GetObject" };
    let object = found(request)?;
    if object.marker {
        return Err(method_not_allowed());
    }
    let modified = object.modified / 1000;
    let date = |name: &str| request.header(name).and_then(clock::parse_date);
    if request
        .header("if-match")
        .is_some_and(|h| !names_etag(h, &object.etag))
        || date("if-unmodified-since").is_some_and(|since| modified > since)
    {
        return Err(Fault::new(
            412,
            "PreconditionFailed",
            "At least one of the pre-conditions you specified did not hold",
        ));
    }
    let unchanged = match request.header("if-none-match") {
        Some(header) => names_etag(header, &object.etag),
        None => date("if-modified-since").is_some_and(|since| modified <= since),
    };
    if unchanged {
        return Ok(describe(Reply::empty(304), &object).operation(operation));
    }

    let part = range(request.header("range"), object.size)?;
    let (start, length) = part.unwrap_or((0, object.size));
    let mut reply = describe(
        Reply::empty(if part.is_some() { 206 } else { 200 }),
        &object,
    );
    if part.is_some() {
        reply = reply.header(
            "content-range",
            &format!("bytes {start}-{}/{}", start + length - 1, object.size),
        );
    }
    // What a presigned URL may ask the answer to say about itself.
    for (query, header) in [
        ("response-content-type", "content-type"),
        ("response-content-disposition", "content-disposition"),
        ("response-content-encoding", "content-encoding"),
        ("response-content-language", "content-language"),
        ("response-cache-control", "cache-control"),
        ("response-expires", "expires"),
    ] {
        if let Some(value) = request.query(query) {
            reply.headers.retain(|(name, _)| name != header);
            reply = reply.header(header, value);
        }
    }
    reply = if head {
        // No body, but its length all the same.
        reply.header("content-length", &length.to_string())
    } else if length == 0 {
        reply
    } else {
        reply.with_file(FileBody {
            path: files::path(request.store, &object.file),
            start,
            length,
        })
    };
    Ok(reply.operation(operation))
}

pub(super) async fn put(request: &Request<'_>, upload: Upload) -> Result<Reply, Fault> {
    // Before anything is written.
    records::bucket(request.store, &request.bucket).map_err(fault)?;
    if request
        .header("if-none-match")
        .is_some_and(|h| h.trim() == "*")
        && records::get(request.store, &request.bucket, &request.key, None).is_ok()
    {
        return Err(Fault::new(
            412,
            "PreconditionFailed",
            "At least one of the pre-conditions you specified did not hold",
        ));
    }
    let written = save(request.store, upload, &request.call.headers).await?;
    let stored = records::put(
        request.store,
        &request.bucket,
        &request.key,
        &written,
        &written.md5,
        &meta(&request.call.headers),
    );
    let object = stored.map_err(|problem| {
        files::remove(request.store, &written.file);
        fault(problem)
    })?;
    let reply = Reply::empty(200).header("etag", &quoted(&object.etag));
    Ok(version_header(reply, &object).operation("PutObject"))
}

/// The object an `x-amz-copy-source` header names: `/bucket/key`, maybe
/// with `?versionId=`.
pub(super) fn source(request: &Request) -> Result<Object, Fault> {
    let header = request.header("x-amz-copy-source").unwrap_or_default();
    let (path, version) = match header.split_once("?versionId=") {
        Some((path, version)) => (path, Some(version)),
        None => (header, None),
    };
    let path = decode_path(path.trim_start_matches('/'));
    let (bucket, key) = path.split_once('/').ok_or_else(|| {
        Fault::bad_request(
            "InvalidArgument",
            "Copy Source must mention the source bucket and key: sourcebucket/sourcekey",
        )
    })?;
    let object = records::get(request.store, bucket, key, version).map_err(fault)?;
    if object.marker {
        return Err(fault(Problem::NoObject {
            key: key.to_owned(),
            marker: Some(object.version),
        }));
    }
    Ok(object)
}

async fn copy(request: &Request<'_>) -> Result<Reply, Fault> {
    let from = source(request)?;
    records::bucket(request.store, &request.bucket).map_err(fault)?;
    let internal = |why: String| Fault::new(500, "InternalError", why);
    let written = files::copy(request.store, &from.file, 0, from.size)
        .await
        .map_err(internal)?;

    // What's kept with the copy: the source's, unless told to replace it.
    let replace = |header: &str| {
        request
            .header(header)
            .is_some_and(|d| d.eq_ignore_ascii_case("REPLACE"))
    };
    let sent = meta(&request.call.headers);
    let mut kept = if replace("x-amz-metadata-directive") {
        sent.clone()
    } else {
        from.meta.clone()
    };
    kept.tags = if replace("x-amz-tagging-directive") {
        sent.tags
    } else {
        from.meta.tags.clone()
    };
    let stored = records::put(
        request.store,
        &request.bucket,
        &request.key,
        &written,
        &written.md5,
        &kept,
    );
    let object = stored.map_err(|problem| {
        files::remove(request.store, &written.file);
        fault(problem)
    })?;
    let mut reply = xml_reply(
        "CopyObjectResult",
        &format!(
            "{}{}",
            xml::element("ETag", &quoted(&object.etag)),
            xml::element("LastModified", &clock::iso8601_millis(object.modified)),
        ),
    );
    if from.version != records::NO_VERSION {
        reply = reply.header("x-amz-copy-source-version-id", &from.version);
    }
    Ok(version_header(reply, &object).operation("CopyObject"))
}

fn delete(request: &Request) -> Result<Reply, Fault> {
    let version = request.query("versionId").filter(|v| !v.is_empty());
    let deleted =
        records::delete(request.store, &request.bucket, &request.key, version).map_err(fault)?;
    let mut reply = Reply::empty(204);
    if let Some(version) = &deleted.version {
        reply = reply.header("x-amz-version-id", version);
    }
    if deleted.marker {
        reply = reply.header("x-amz-delete-marker", "true");
    }
    Ok(reply.operation("DeleteObject"))
}

/// Deletes the objects a document lists, saying how each went.
pub(super) fn delete_many(request: &Request) -> Result<Reply, Fault> {
    let document = request.document();
    let quiet = xml::text(&document, "Quiet").is_some_and(|q| q == "true");
    let mut out = String::new();
    for object in xml::blocks(&document, "Object") {
        let Some(key) = xml::text(object, "Key") else {
            continue;
        };
        let version = xml::text(object, "VersionId").filter(|v| !v.is_empty());
        match records::delete(request.store, &request.bucket, &key, version.as_deref()) {
            Ok(_) if quiet => {}
            Ok(deleted) => {
                let mut one = xml::element("Key", &key);
                match (&deleted.version, deleted.marker, &version) {
                    // A marker was made: its version is the news.
                    (Some(made), true, None) => {
                        one += "<DeleteMarker>true</DeleteMarker>";
                        one += &xml::element("DeleteMarkerVersionId", made);
                    }
                    (Some(gone), marker, Some(_)) => {
                        one += &xml::element("VersionId", gone);
                        if marker {
                            one += "<DeleteMarker>true</DeleteMarker>";
                        }
                    }
                    _ => {}
                }
                out += &format!("<Deleted>{one}</Deleted>");
            }
            Err(problem) => {
                let error = fault(problem);
                out += &format!(
                    "<Error>{}{}{}</Error>",
                    xml::element("Key", &key),
                    xml::element("Code", &error.code),
                    xml::element("Message", &error.message),
                );
            }
        }
    }
    Ok(xml_reply("DeleteResult", &out).operation("DeleteObjects"))
}

fn get_tagging(request: &Request) -> Result<Reply, Fault> {
    let object = found(request)?;
    let tags: String = object
        .meta
        .tags
        .iter()
        .map(|(key, value)| {
            format!(
                "<Tag>{}{}</Tag>",
                xml::element("Key", key),
                xml::element("Value", value)
            )
        })
        .collect();
    let reply = xml_reply("Tagging", &format!("<TagSet>{tags}</TagSet>"));
    Ok(version_header(reply, &object).operation("GetObjectTagging"))
}

fn set_tags(
    request: &Request,
    tags: BTreeMap<String, String>,
    status: u16,
    operation: &'static str,
) -> Result<Reply, Fault> {
    let object = found(request)?;
    let mut kept = object.meta.clone();
    kept.tags = tags;
    records::set_meta(request.store, &object, &kept).map_err(fault)?;
    Ok(version_header(Reply::empty(status), &object).operation(operation))
}

/// The tags an `x-amz-tagging` header carries, for uploads in parts.
pub(super) fn header_tags(request: &Request) -> BTreeMap<String, String> {
    request
        .header("x-amz-tagging")
        .map(tags_in_header)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_ranges() {
        let of = |header: &str, size| range(Some(header), size);
        assert_eq!(of("bytes=0-4", 10).unwrap(), Some((0, 5)));
        assert_eq!(of("bytes=5-", 10).unwrap(), Some((5, 5)));
        assert_eq!(of("bytes=-3", 10).unwrap(), Some((7, 3)));
        assert_eq!(of("bytes=-30", 10).unwrap(), Some((0, 10)));
        // Past the end is cut to it; starting past it is an error.
        assert_eq!(of("bytes=8-99", 10).unwrap(), Some((8, 2)));
        assert_eq!(of("bytes=10-12", 10).unwrap_err().code, "InvalidRange");
        assert_eq!(of("bytes=0-0", 0).unwrap_err().code, "InvalidRange");
        // Not understood: the whole object.
        assert_eq!(of("bytes=0-1,4-5", 10).unwrap(), None);
        assert_eq!(of("lines=1-2", 10).unwrap(), None);
        assert_eq!(of("bytes=5-2", 10).unwrap(), None);
        assert_eq!(range(None, 10).unwrap(), None);
    }

    #[test]
    fn matches_etags() {
        assert!(names_etag("\"abc\"", "abc"));
        assert!(names_etag("abc", "abc"));
        assert!(names_etag("\"x\", W/\"abc\"", "abc"));
        assert!(names_etag("*", "abc"));
        assert!(!names_etag("\"abd\"", "abc"));
    }
}
