//! Uploads in parts: how the CLI and SDKs send anything large. An upload
//! is started, its parts sent in any order (or copied from other
//! objects), and then completed into one object, or aborted.

use super::{Request, fault, meta, objects, quoted, save, version_header, xml, xml_reply};
use crate::clock;
use crate::objects::{Written, files, records};
use crate::protocol::{Fault, Reply};
use crate::service::Upload;

/// The most parts an upload may have.
const MAX_PARTS: u32 = 10_000;

fn upload_id<'a>(request: &'a Request) -> &'a str {
    request.query("uploadId").unwrap_or_default()
}

fn part_number(request: &Request) -> Result<u32, Fault> {
    request
        .query("partNumber")
        .and_then(|n| n.parse().ok())
        .filter(|n| (1..=MAX_PARTS).contains(n))
        .ok_or_else(|| {
            Fault::bad_request(
                "InvalidArgument",
                "Part number must be an integer between 1 and 10000, inclusive",
            )
        })
}

pub(super) fn create(request: &Request) -> Result<Reply, Fault> {
    let mut kept = meta(&request.call.headers);
    kept.tags = objects::header_tags(request);
    let upload = records::create_upload(request.store, &request.bucket, &request.key, &kept)
        .map_err(fault)?;
    Ok(xml_reply(
        "InitiateMultipartUploadResult",
        &format!(
            "{}{}{}",
            xml::element("Bucket", &upload.bucket),
            xml::element("Key", &upload.key),
            xml::element("UploadId", &upload.id),
        ),
    )
    .operation("CreateMultipartUpload"))
}

/// Keeps a written file as the part the request names.
fn keep_part(request: &Request, number: u32, written: &Written) -> Result<(), Fault> {
    records::put_part(request.store, upload_id(request), number, written).map_err(|problem| {
        files::remove(request.store, &written.file);
        fault(problem)
    })
}

pub(super) async fn upload_part(request: &Request<'_>, upload: Upload) -> Result<Reply, Fault> {
    let number = part_number(request)?;
    // Before anything is written.
    records::upload(request.store, upload_id(request)).map_err(fault)?;
    let written = save(request.store, upload, &request.call.headers).await?;
    keep_part(request, number, &written)?;
    Ok(Reply::empty(200)
        .header("etag", &quoted(&written.md5))
        .operation("UploadPart"))
}

/// A part made of another object, or of a range of one.
pub(super) async fn upload_part_copy(request: &Request<'_>) -> Result<Reply, Fault> {
    let number = part_number(request)?;
    records::upload(request.store, upload_id(request)).map_err(fault)?;
    let from = objects::source(request)?;
    let whole = (0, from.size);
    let (start, length) = match request
        .header("x-amz-copy-source-range")
        .and_then(|range| range.trim().strip_prefix("bytes="))
        .and_then(|range| range.split_once('-'))
    {
        Some((first, last)) => match (first.parse::<u64>(), last.parse::<u64>()) {
            (Ok(first), Ok(last)) if first <= last && last < from.size => (first, last - first + 1),
            _ => {
                return Err(Fault::bad_request(
                    "InvalidArgument",
                    "The x-amz-copy-source-range value must be of the form bytes=first-last where first and \
                     last are the zero-based offsets of the first and last bytes to copy",
                ));
            }
        },
        None => whole,
    };
    let written = files::copy(request.store, &from.file, start, length)
        .await
        .map_err(|why| Fault::new(500, "InternalError", why))?;
    keep_part(request, number, &written)?;
    Ok(xml_reply(
        "CopyPartResult",
        &format!(
            "{}{}",
            xml::element("ETag", &quoted(&written.md5)),
            xml::element("LastModified", &clock::iso8601_millis(clock::now_millis())),
        ),
    )
    .operation("UploadPartCopy"))
}

pub(super) async fn complete(request: &Request<'_>) -> Result<Reply, Fault> {
    let document = request.document();
    let chosen: Vec<(u32, String)> = xml::blocks(&document, "Part")
        .into_iter()
        .filter_map(|part| {
            let number = xml::text(part, "PartNumber")?.parse().ok()?;
            Some((number, xml::text(part, "ETag").unwrap_or_default()))
        })
        .collect();
    let object = records::complete_upload(request.store, upload_id(request), &chosen)
        .await
        .map_err(fault)?;
    let reply = xml_reply(
        "CompleteMultipartUploadResult",
        &format!(
            "{}{}{}{}",
            xml::element("Location", &format!("/{}/{}", object.bucket, object.key)),
            xml::element("Bucket", &object.bucket),
            xml::element("Key", &object.key),
            xml::element("ETag", &quoted(&object.etag)),
        ),
    );
    Ok(version_header(reply, &object).operation("CompleteMultipartUpload"))
}

pub(super) fn abort(request: &Request) -> Result<Reply, Fault> {
    records::remove_upload(request.store, upload_id(request)).map_err(fault)?;
    Ok(Reply::empty(204).operation("AbortMultipartUpload"))
}

pub(super) fn list_parts(request: &Request) -> Result<Reply, Fault> {
    let upload = records::upload(request.store, upload_id(request)).map_err(fault)?;
    let parts: String = records::parts(request.store, &upload.id)
        .map_err(fault)?
        .iter()
        .map(|part| {
            format!(
                "<Part>{}{}{}{}</Part>",
                xml::element("PartNumber", &part.number.to_string()),
                xml::element("LastModified", &clock::iso8601_millis(part.modified)),
                xml::element("ETag", &quoted(&part.etag)),
                xml::element("Size", &part.size.to_string()),
            )
        })
        .collect();
    Ok(xml_reply(
        "ListPartsResult",
        &format!(
            "{}{}{}<PartNumberMarker>0</PartNumberMarker><MaxParts>{MAX_PARTS}</MaxParts>\
             <IsTruncated>false</IsTruncated><StorageClass>STANDARD</StorageClass>{parts}",
            xml::element("Bucket", &upload.bucket),
            xml::element("Key", &upload.key),
            xml::element("UploadId", &upload.id),
        ),
    )
    .operation("ListParts"))
}

pub(super) fn list_uploads(request: &Request) -> Result<Reply, Fault> {
    let prefix = request.query("prefix").unwrap_or_default();
    let uploads: String = records::uploads(request.store, &request.bucket)
        .map_err(fault)?
        .iter()
        .filter(|upload| upload.key.starts_with(prefix))
        .map(|upload| {
            format!(
                "<Upload>{}{}{}<StorageClass>STANDARD</StorageClass></Upload>",
                xml::element("Key", &upload.key),
                xml::element("UploadId", &upload.id),
                xml::element("Initiated", &clock::iso8601_millis(upload.started)),
            )
        })
        .collect();
    Ok(xml_reply(
        "ListMultipartUploadsResult",
        &format!(
            "{}{}<KeyMarker></KeyMarker><UploadIdMarker></UploadIdMarker><MaxUploads>1000</MaxUploads>\
             <IsTruncated>false</IsTruncated>{uploads}",
            xml::element("Bucket", &request.bucket),
            xml::element("Prefix", prefix),
        ),
    )
    .operation("ListMultipartUploads"))
}
