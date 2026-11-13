//! Buckets: making and removing them, versioning, and the documents kept
//! with one (CORS rules, a policy, lifecycle rules…). Those documents are
//! stored and given back as they came, so tools that set them and read
//! them back are satisfied; none of them changes what the emulator does.

use hyper::Method;

use super::{Request, fault, list, method_not_allowed, multipart, objects, xml, xml_reply};
use crate::objects::{Versioning, records};
use crate::protocol::{Fault, Reply};

/// Who owns everything, as listings and ACLs name them.
pub const OWNER_ID: &str = "75aa57f09aa0c8caeab4f8c24e99d10f8e7faeebf76c078efc7c6caea54ba06a";
pub const OWNER_NAME: &str = "ferrocloud";

/// A kind of document kept with a bucket.
struct Kept {
    /// The query parameter that names it: `?cors`.
    query: &'static str,
    get: &'static str,
    put: &'static str,
    delete: &'static str,
    /// What reading it says when there's none: an error's code, or the
    /// document a bucket has by default.
    missing: Missing,
}

enum Missing {
    Error(&'static str),
    Default(&'static str),
}

const KEPT: &[Kept] = &[
    Kept {
        query: "cors",
        get: "GetBucketCors",
        put: "PutBucketCors",
        delete: "DeleteBucketCors",
        missing: Missing::Error("NoSuchCORSConfiguration"),
    },
    Kept {
        query: "policy",
        get: "GetBucketPolicy",
        put: "PutBucketPolicy",
        delete: "DeleteBucketPolicy",
        missing: Missing::Error("NoSuchBucketPolicy"),
    },
    Kept {
        query: "tagging",
        get: "GetBucketTagging",
        put: "PutBucketTagging",
        delete: "DeleteBucketTagging",
        missing: Missing::Error("NoSuchTagSet"),
    },
    Kept {
        query: "lifecycle",
        get: "GetBucketLifecycleConfiguration",
        put: "PutBucketLifecycleConfiguration",
        delete: "DeleteBucketLifecycle",
        missing: Missing::Error("NoSuchLifecycleConfiguration"),
    },
    Kept {
        query: "encryption",
        get: "GetBucketEncryption",
        put: "PutBucketEncryption",
        delete: "DeleteBucketEncryption",
        missing: Missing::Error("ServerSideEncryptionConfigurationNotFoundError"),
    },
    Kept {
        query: "publicAccessBlock",
        get: "GetPublicAccessBlock",
        put: "PutPublicAccessBlock",
        delete: "DeletePublicAccessBlock",
        missing: Missing::Error("NoSuchPublicAccessBlockConfiguration"),
    },
    Kept {
        query: "ownershipControls",
        get: "GetBucketOwnershipControls",
        put: "PutBucketOwnershipControls",
        delete: "DeleteBucketOwnershipControls",
        missing: Missing::Error("OwnershipControlsNotFoundError"),
    },
    Kept {
        query: "website",
        get: "GetBucketWebsite",
        put: "PutBucketWebsite",
        delete: "DeleteBucketWebsite",
        missing: Missing::Error("NoSuchWebsiteConfiguration"),
    },
    Kept {
        query: "replication",
        get: "GetBucketReplication",
        put: "PutBucketReplication",
        delete: "DeleteBucketReplication",
        missing: Missing::Error("ReplicationConfigurationNotFoundError"),
    },
    Kept {
        query: "object-lock",
        get: "GetObjectLockConfiguration",
        put: "PutObjectLockConfiguration",
        delete: "PutObjectLockConfiguration",
        missing: Missing::Error("ObjectLockConfigurationNotFoundError"),
    },
    Kept {
        query: "notification",
        get: "GetBucketNotificationConfiguration",
        put: "PutBucketNotificationConfiguration",
        delete: "PutBucketNotificationConfiguration",
        missing: Missing::Default("NotificationConfiguration"),
    },
    Kept {
        query: "logging",
        get: "GetBucketLogging",
        put: "PutBucketLogging",
        delete: "PutBucketLogging",
        missing: Missing::Default("BucketLoggingStatus"),
    },
    Kept {
        query: "accelerate",
        get: "GetBucketAccelerateConfiguration",
        put: "PutBucketAccelerateConfiguration",
        delete: "PutBucketAccelerateConfiguration",
        missing: Missing::Default("AccelerateConfiguration"),
    },
    Kept {
        query: "requestPayment",
        get: "GetBucketRequestPayment",
        put: "PutBucketRequestPayment",
        delete: "PutBucketRequestPayment",
        missing: Missing::Default("RequestPaymentConfiguration"),
    },
];

/// The ACL everything has: its owner, in full control.
pub fn acl() -> String {
    let who = format!(
        "{}{}",
        xml::element("ID", OWNER_ID),
        xml::element("DisplayName", OWNER_NAME)
    );
    xml::document(
        "AccessControlPolicy",
        &format!(
            "<Owner>{who}</Owner><AccessControlList><Grant>\
             <Grantee xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xsi:type=\"CanonicalUser\">{who}</Grantee>\
             <Permission>FULL_CONTROL</Permission></Grant></AccessControlList>"
        ),
    )
}

/// Bucket names as S3 allows them: 3 to 63 of lowercase letters, digits,
/// dots and hyphens, starting and ending with a letter or digit.
fn valid_name(name: &str) -> bool {
    let edge = |b: u8| b.is_ascii_lowercase() || b.is_ascii_digit();
    let bytes = name.as_bytes();
    (3..=63).contains(&bytes.len())
        && bytes.iter().all(|b| edge(*b) || matches!(b, b'.' | b'-'))
        && edge(bytes[0])
        && edge(bytes[bytes.len() - 1])
}

pub(super) async fn handle(request: &Request<'_>, method: &Method) -> Result<Reply, Fault> {
    let kept = KEPT.iter().find(|kept| request.has(kept.query));
    match *method {
        Method::PUT => {
            if request.has("versioning") {
                put_versioning(request)
            } else if let Some(kept) = kept {
                put_kept(request, kept)
            } else if request.has("acl") {
                records::bucket(request.store, &request.bucket).map_err(fault)?;
                Ok(Reply::empty(200).operation("PutBucketAcl"))
            } else {
                create(request)
            }
        }
        Method::GET => {
            if request.has("location") {
                location(request)
            } else if request.has("versioning") {
                get_versioning(request)
            } else if request.has("versions") {
                list::versions(request)
            } else if request.has("uploads") {
                multipart::list_uploads(request)
            } else if request.has("acl") {
                records::bucket(request.store, &request.bucket).map_err(fault)?;
                Ok(Reply::new(200, "application/xml", acl()).operation("GetBucketAcl"))
            } else if let Some(kept) = kept {
                get_kept(request, kept)
            } else if request.query("list-type") == Some("2") {
                list::objects_v2(request)
            } else {
                list::objects_v1(request)
            }
        }
        Method::HEAD => {
            let bucket = records::bucket(request.store, &request.bucket).map_err(fault)?;
            Ok(Reply::empty(200)
                .header("x-amz-bucket-region", &bucket.region)
                .operation("HeadBucket"))
        }
        Method::DELETE => match kept {
            Some(kept) => {
                records::set_config(request.store, &request.bucket, kept.query, None)
                    .map_err(fault)?;
                Ok(Reply::empty(204).operation(kept.delete))
            }
            None => {
                records::delete_bucket(request.store, &request.bucket).map_err(fault)?;
                Ok(Reply::empty(204).operation("DeleteBucket"))
            }
        },
        Method::POST if request.has("delete") => objects::delete_many(request),
        _ => Err(method_not_allowed()),
    }
}

fn create(request: &Request) -> Result<Reply, Fault> {
    if !valid_name(&request.bucket) {
        return Err(Fault::bad_request(
            "InvalidBucketName",
            "The specified bucket is not valid.",
        ));
    }
    // Where it's asked to be, else where the request was signed for.
    let region = xml::text(&request.document(), "LocationConstraint")
        .filter(|region| !region.is_empty())
        .unwrap_or_else(|| request.call.route.region.clone());
    records::create_bucket(request.store, &request.bucket, &region).map_err(fault)?;
    Ok(Reply::empty(200)
        .header("location", &format!("/{}", request.bucket))
        .operation("CreateBucket"))
}

fn location(request: &Request) -> Result<Reply, Fault> {
    let bucket = records::bucket(request.store, &request.bucket).map_err(fault)?;
    // The first region is told by saying nothing, as S3 does.
    let region = if bucket.region == "us-east-1" {
        ""
    } else {
        &bucket.region
    };
    let body = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<LocationConstraint xmlns=\"{}\">{region}</LocationConstraint>",
        xml::NAMESPACE
    );
    Ok(Reply::new(200, "application/xml", body).operation("GetBucketLocation"))
}

fn put_versioning(request: &Request) -> Result<Reply, Fault> {
    let versioning = match xml::text(&request.document(), "Status").as_deref() {
        Some("Enabled") => Versioning::Enabled,
        Some("Suspended") => Versioning::Suspended,
        _ => {
            return Err(Fault::bad_request(
                "MalformedXML",
                "The versioning status must be Enabled or Suspended.",
            ));
        }
    };
    records::set_versioning(request.store, &request.bucket, versioning).map_err(fault)?;
    Ok(Reply::empty(200).operation("PutBucketVersioning"))
}

fn get_versioning(request: &Request) -> Result<Reply, Fault> {
    let bucket = records::bucket(request.store, &request.bucket).map_err(fault)?;
    let status = match bucket.versioning {
        Versioning::Off => String::new(),
        Versioning::Enabled => xml::element("Status", "Enabled"),
        Versioning::Suspended => xml::element("Status", "Suspended"),
    };
    Ok(xml_reply("VersioningConfiguration", &status).operation("GetBucketVersioning"))
}

fn content_type(kept: &Kept) -> &'static str {
    if kept.query == "policy" {
        "application/json"
    } else {
        "application/xml"
    }
}

fn put_kept(request: &Request, kept: &Kept) -> Result<Reply, Fault> {
    let document = request.document();
    if kept.query == "policy" && serde_json::from_str::<serde_json::Value>(&document).is_err() {
        return Err(Fault::bad_request(
            "MalformedPolicy",
            "Policies must be valid JSON and the first byte must be '{'",
        ));
    }
    records::set_config(request.store, &request.bucket, kept.query, Some(&document))
        .map_err(fault)?;
    // A policy is answered with 204, the others with 200.
    let status = if kept.query == "policy" { 204 } else { 200 };
    Ok(Reply::empty(status).operation(kept.put))
}

fn get_kept(request: &Request, kept: &Kept) -> Result<Reply, Fault> {
    let document = records::config(request.store, &request.bucket, kept.query).map_err(fault)?;
    let document = match (document, &kept.missing) {
        (Some(document), _) => document,
        (None, Missing::Error(code)) => {
            return Err(Fault::new(
                404,
                code,
                format!("The bucket has no {} configuration.", kept.query),
            ));
        }
        (None, Missing::Default(root)) => {
            let inner = if kept.query == "requestPayment" {
                xml::element("Payer", "BucketOwner")
            } else {
                String::new()
            };
            xml::document(root, &inner)
        }
    };
    Ok(Reply::new(200, content_type(kept), document).operation(kept.get))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks_bucket_names() {
        for good in ["abc", "my-bucket", "my.bucket.2026", "0bucket9"] {
            assert!(valid_name(good), "{good}");
        }
        for bad in [
            "ab",
            "My-Bucket",
            "-bucket",
            "bucket-",
            "under_score",
            &"a".repeat(64),
        ] {
            assert!(!valid_name(bad), "{bad}");
        }
    }

    #[test]
    fn names_each_kept_document_once() {
        let mut queries: Vec<&str> = KEPT.iter().map(|k| k.query).collect();
        queries.sort_unstable();
        queries.dedup();
        assert_eq!(queries.len(), KEPT.len());
        assert!(acl().contains("FULL_CONTROL"));
    }
}
