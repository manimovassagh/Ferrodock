//! Listing: the account's buckets, a bucket's objects (both versions of
//! the API), and its versions.

use super::buckets::{OWNER_ID, OWNER_NAME};
use super::{Request, fault, quoted, url_encode, xml, xml_reply};
use crate::clock;
use crate::objects::{Object, records};
use crate::protocol::{Fault, Reply};

/// The most a page holds, whatever was asked for.
const MAX_KEYS: usize = 1000;

fn owner() -> String {
    format!(
        "<Owner>{}{}</Owner>",
        xml::element("ID", OWNER_ID),
        xml::element("DisplayName", OWNER_NAME)
    )
}

pub(super) fn buckets(request: &Request) -> Result<Reply, Fault> {
    let listed: String = records::buckets(request.store)
        .map_err(fault)?
        .iter()
        .map(|bucket| {
            format!(
                "<Bucket>{}{}{}</Bucket>",
                xml::element("Name", &bucket.name),
                xml::element("CreationDate", &clock::iso8601_millis(bucket.created)),
                xml::element("BucketRegion", &bucket.region),
            )
        })
        .collect();
    Ok(xml_reply(
        "ListAllMyBucketsResult",
        &format!("{}<Buckets>{listed}</Buckets>", owner()),
    )
    .operation("ListBuckets"))
}

/// What every listing is asked: under which prefix, rolled up at which
/// delimiter, how many, and whether names are to be URL-encoded.
struct Asked<'a> {
    prefix: &'a str,
    delimiter: &'a str,
    max: usize,
    encode: bool,
}

impl<'a> Asked<'a> {
    fn of(request: &'a Request) -> Self {
        Self {
            prefix: request.query("prefix").unwrap_or_default(),
            delimiter: request.query("delimiter").unwrap_or_default(),
            max: request
                .query("max-keys")
                .and_then(|max| max.parse().ok())
                .map_or(MAX_KEYS, |max: usize| max.min(MAX_KEYS)),
            encode: request.query("encoding-type") == Some("url"),
        }
    }

    /// A key or prefix as the reply carries it.
    fn name(&self, tag: &str, text: &str) -> String {
        if self.encode {
            xml::element(tag, &url_encode(text))
        } else {
            xml::element(tag, text)
        }
    }

    /// What every listing's reply starts with.
    fn head(&self, bucket: &str) -> String {
        let mut out = xml::element("Name", bucket) + &self.name("Prefix", self.prefix);
        if !self.delimiter.is_empty() {
            out += &self.name("Delimiter", self.delimiter);
        }
        out += &xml::element("MaxKeys", &self.max.to_string());
        if self.encode {
            out += &xml::element("EncodingType", "url");
        }
        out
    }

    fn prefixes(&self, prefixes: &[String]) -> String {
        prefixes
            .iter()
            .map(|prefix| {
                format!(
                    "<CommonPrefixes>{}</CommonPrefixes>",
                    self.name("Prefix", prefix)
                )
            })
            .collect()
    }

    fn contents(&self, object: &Object, with_owner: bool) -> String {
        format!(
            "<Contents>{}{}{}{}<StorageClass>STANDARD</StorageClass>{}</Contents>",
            self.name("Key", &object.key),
            xml::element("LastModified", &clock::iso8601_millis(object.modified)),
            xml::element("ETag", &quoted(&object.etag)),
            xml::element("Size", &object.size.to_string()),
            if with_owner { owner() } else { String::new() },
        )
    }
}

pub(super) fn objects_v2(request: &Request) -> Result<Reply, Fault> {
    let asked = Asked::of(request);
    let token = request.query("continuation-token");
    let start_after = request.query("start-after").unwrap_or_default();
    let after = token.unwrap_or(start_after);
    let listing = records::list(
        request.store,
        &request.bucket,
        asked.prefix,
        asked.delimiter,
        after,
        asked.max,
    )
    .map_err(fault)?;

    let mut out = asked.head(&request.bucket);
    out += &xml::element(
        "KeyCount",
        &(listing.objects.len() + listing.prefixes.len()).to_string(),
    );
    out += &xml::element("IsTruncated", &listing.truncated.to_string());
    if let Some(token) = token {
        out += &xml::element("ContinuationToken", token);
    }
    if !start_after.is_empty() {
        out += &asked.name("StartAfter", start_after);
    }
    if let Some(next) = &listing.next {
        out += &xml::element("NextContinuationToken", next);
    }
    let with_owner = request.query("fetch-owner") == Some("true");
    out += &listing
        .objects
        .iter()
        .map(|o| asked.contents(o, with_owner))
        .collect::<String>();
    out += &asked.prefixes(&listing.prefixes);
    Ok(xml_reply("ListBucketResult", &out).operation("ListObjectsV2"))
}

pub(super) fn objects_v1(request: &Request) -> Result<Reply, Fault> {
    let asked = Asked::of(request);
    let marker = request.query("marker").unwrap_or_default();
    let listing = records::list(
        request.store,
        &request.bucket,
        asked.prefix,
        asked.delimiter,
        marker,
        asked.max,
    )
    .map_err(fault)?;

    let mut out = asked.head(&request.bucket);
    out += &asked.name("Marker", marker);
    out += &xml::element("IsTruncated", &listing.truncated.to_string());
    if let Some(next) = &listing.next {
        out += &asked.name("NextMarker", next);
    }
    out += &listing
        .objects
        .iter()
        .map(|o| asked.contents(o, true))
        .collect::<String>();
    out += &asked.prefixes(&listing.prefixes);
    Ok(xml_reply("ListBucketResult", &out).operation("ListObjects"))
}

pub(super) fn versions(request: &Request) -> Result<Reply, Fault> {
    let asked = Asked::of(request);
    let key_marker = request.query("key-marker").unwrap_or_default();
    let version_marker = request.query("version-id-marker").filter(|v| !v.is_empty());
    let listing = records::list_versions(
        request.store,
        &request.bucket,
        asked.prefix,
        asked.delimiter,
        (key_marker, version_marker),
        asked.max,
    )
    .map_err(fault)?;

    let mut out = asked.head(&request.bucket);
    out += &asked.name("KeyMarker", key_marker);
    out += &xml::element("VersionIdMarker", version_marker.unwrap_or_default());
    out += &xml::element("IsTruncated", &listing.truncated.to_string());
    if let Some((key, version)) = &listing.next {
        out += &asked.name("NextKeyMarker", key);
        out += &xml::element("NextVersionIdMarker", version);
    }
    for version in &listing.versions {
        let common = format!(
            "{}{}{}{}",
            asked.name("Key", &version.key),
            xml::element("VersionId", &version.version),
            xml::element("IsLatest", &version.latest.to_string()),
            xml::element("LastModified", &clock::iso8601_millis(version.modified)),
        );
        out += &if version.marker {
            format!("<DeleteMarker>{common}{}</DeleteMarker>", owner())
        } else {
            format!(
                "<Version>{common}{}{}<StorageClass>STANDARD</StorageClass>{}</Version>",
                xml::element("ETag", &quoted(&version.etag)),
                xml::element("Size", &version.size.to_string()),
                owner(),
            )
        };
    }
    out += &asked.prefixes(&listing.prefixes);
    Ok(xml_reply("ListVersionsResult", &out).operation("ListObjectVersions"))
}
