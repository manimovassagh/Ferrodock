//! What the request log keeps of a request and its reply: headers and
//! the start of each body, with secrets masked. Kept for people to read,
//! so nothing here is used to answer a request.

use hyper::HeaderMap;
use serde::Serialize;

use crate::protocol::Protocol;

/// How much of a body is kept.
pub const BODY_LIMIT: usize = 16 * 1024;

/// What a masked value reads as.
const MASK: &str = "***";

/// Fields whose values are secrets, wherever they appear in a body.
const SECRETS: &[&str] = &[
    "SecretAccessKey",
    "SessionToken",
    "Password",
    "NewPassword",
    "OldPassword",
    "MasterUserPassword",
    "SecretString",
    "SecretBinary",
    "AuthToken",
    "WebIdentityToken",
];

/// A body as the log keeps it.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct Captured {
    /// Its text, up to [`BODY_LIMIT`]; empty for one that isn't text.
    pub text: String,
    /// How long the whole body was, in bytes.
    pub size: u64,
    /// More was sent than is kept.
    pub truncated: bool,
    /// It isn't text (an upload, say), so only its size is kept.
    pub binary: bool,
}

/// Everything kept of one request besides its line in the log.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct Detail {
    pub request_headers: Vec<(String, String)>,
    pub request_body: Captured,
    pub response_headers: Vec<(String, String)>,
    pub response_body: Captured,
}

/// Keeps the start of `bytes` if it's text, masked; else just its size.
pub fn body(bytes: &[u8]) -> Captured {
    let size = bytes.len() as u64;
    let kept = &bytes[..bytes.len().min(BODY_LIMIT)];
    let truncated = kept.len() < bytes.len();
    let text = match std::str::from_utf8(kept) {
        Ok(text) => Some(text),
        // Cut in the middle of a character: text all the same.
        Err(e) if truncated && e.error_len().is_none() => {
            std::str::from_utf8(&kept[..e.valid_up_to()]).ok()
        }
        Err(_) => None,
    };
    match text {
        Some(text) if !text.contains('\0') => Captured {
            text: mask(text),
            size,
            truncated,
            binary: false,
        },
        _ => Captured {
            size,
            binary: true,
            ..Captured::default()
        },
    }
}

/// The headers, in order, with what signs or authorises a request masked.
pub fn headers(map: &HeaderMap) -> Vec<(String, String)> {
    map.iter()
        .map(|(name, value)| {
            let value = value.to_str().unwrap_or("(not text)");
            let value = match name.as_str() {
                "x-amz-security-token" | "cookie" | "set-cookie" => MASK.to_owned(),
                // Who signed and for what stays readable; the signature
                // itself doesn't.
                "authorization" => match value.split_once("Signature=") {
                    Some((before, _)) => format!("{before}Signature={MASK}"),
                    None => MASK.to_owned(),
                },
                _ => value.to_owned(),
            };
            (name.as_str().to_owned(), value)
        })
        .collect()
}

/// `text` with the value of every secret field replaced, in JSON, XML and
/// form bodies alike.
pub fn mask(text: &str) -> String {
    let mut out = text.to_owned();
    for name in SECRETS {
        if !out.contains(name) {
            continue;
        }
        out = mask_between(&out, &format!("<{name}>"), |rest| rest.find("</"));
        out = mask_between(&out, &format!("&{name}="), |rest| {
            Some(rest.find('&').unwrap_or(rest.len()))
        });
        if let Some(rest) = out.strip_prefix(&format!("{name}=")) {
            let end = rest.find('&').unwrap_or(rest.len());
            out = format!("{name}={MASK}{}", &rest[end..]);
        }
        out = mask_json(&out, name);
    }
    out
}

/// Replaces what follows each `start`, up to where `end` says it stops.
fn mask_between(text: &str, start: &str, end: impl Fn(&str) -> Option<usize>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(start) {
        let value = &rest[at + start.len()..];
        let Some(len) = end(value) else { break };
        out.push_str(&rest[..at + start.len()]);
        out.push_str(MASK);
        rest = &value[len..];
    }
    out + rest
}

/// Replaces the string after each `"name":`.
fn mask_json(text: &str, name: &str) -> String {
    let key = format!("\"{name}\"");
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(&key) {
        let after = &rest[at + key.len()..];
        let value = after.trim_start().strip_prefix(':').map(str::trim_start);
        let Some(inner) = value.and_then(|v| v.strip_prefix('"')) else {
            out.push_str(&rest[..at + key.len()]);
            rest = after;
            continue;
        };
        // The closing quote: the first one not escaped.
        let mut escaped = false;
        let close = inner.char_indices().find_map(|(i, c)| match c {
            _ if escaped => {
                escaped = false;
                None
            }
            '\\' => {
                escaped = true;
                None
            }
            '"' => Some(i),
            _ => None,
        });
        let Some(close) = close else { break };
        let start = rest.len() - inner.len();
        out.push_str(&rest[..start]);
        out.push_str(MASK);
        rest = &inner[close..];
    }
    out + rest
}

/// What a request is about, when its fields say: the first one named like
/// a resource (`RoleName`, `QueueUrl`, `TableName`, `PolicyArn`…).
pub fn resource(protocol: Protocol, query: &[(String, String)], body: &[u8]) -> String {
    let names = |key: &str| {
        !matches!(key, "Action" | "Version")
            && ["Name", "Arn", "Url", "Id"]
                .iter()
                .any(|end| key.ends_with(end))
    };
    let from_pairs = |pairs: &[(String, String)]| {
        pairs
            .iter()
            .find(|(key, value)| names(key) && !value.is_empty())
            .map(|(_, value)| value.clone())
    };
    let found = match protocol {
        Protocol::Query => from_pairs(query)
            .or_else(|| from_pairs(&crate::route::parse_pairs(&String::from_utf8_lossy(body)))),
        Protocol::Json10 | Protocol::Json11 => serde_json::from_slice::<serde_json::Value>(body)
            .ok()
            .and_then(|value| {
                value.as_object()?.iter().find_map(|(key, value)| {
                    value
                        .as_str()
                        .filter(|v| names(key) && !v.is_empty())
                        .map(str::to_owned)
                })
            }),
        // These name their resource in the path, which the log shows.
        Protocol::RestXml | Protocol::RestJson => None,
    };
    found.unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_secrets_in_every_kind_of_body() {
        let xml = "<Credentials><AccessKeyId>ASIA1</AccessKeyId><SecretAccessKey>abc/def</SecretAccessKey>\
                   <SessionToken>tok</SessionToken></Credentials>";
        assert_eq!(
            mask(xml),
            "<Credentials><AccessKeyId>ASIA1</AccessKeyId><SecretAccessKey>***</SecretAccessKey>\
             <SessionToken>***</SessionToken></Credentials>"
        );
        let json = r#"{"Name":"db","SecretString" : "p\"w}","Tags":[],"Password":"x"}"#;
        assert_eq!(
            mask(json),
            r#"{"Name":"db","SecretString" : "***","Tags":[],"Password":"***"}"#
        );
        // A key that isn't a string value is left alone.
        assert_eq!(mask(r#"{"SecretString":null}"#), r#"{"SecretString":null}"#);
        let form =
            "Action=CreateLoginProfile&UserName=dev&Password=hunter2&PasswordResetRequired=false";
        assert_eq!(
            mask(form),
            "Action=CreateLoginProfile&UserName=dev&Password=***&PasswordResetRequired=false"
        );
        assert_eq!(mask("Password=a&UserName=dev"), "Password=***&UserName=dev");
        assert_eq!(
            mask("Action=GetUser&UserName=dev"),
            "Action=GetUser&UserName=dev"
        );
    }

    #[test]
    fn keeps_the_start_of_text_and_the_size_of_the_rest() {
        let small = body(b"Action=GetUser");
        assert_eq!(
            (
                small.text.as_str(),
                small.size,
                small.truncated,
                small.binary
            ),
            ("Action=GetUser", 14, false, false)
        );

        // Cut inside a two-byte character.
        let mut long = "a".repeat(BODY_LIMIT - 1).into_bytes();
        long.extend("éé".as_bytes());
        let cut = body(&long);
        assert!(cut.truncated && !cut.binary);
        assert_eq!(cut.text.len(), BODY_LIMIT - 1);
        assert_eq!(cut.size, long.len() as u64);

        let upload = body(&[0xff, 0xd8, 0xff, 0x00, 0x10]);
        assert!(upload.binary && upload.text.is_empty());
        assert_eq!(upload.size, 5);
    }

    #[test]
    fn masks_what_signs_a_request() {
        let mut map = HeaderMap::new();
        map.insert(
            "authorization",
            "AWS4-HMAC-SHA256 Credential=test/20261112/us-east-1/iam/aws4_request, SignedHeaders=host, Signature=abcdef"
                .parse()
                .unwrap(),
        );
        map.insert("x-amz-security-token", "tok".parse().unwrap());
        map.insert("user-agent", "aws-cli/2".parse().unwrap());
        let kept = headers(&map);
        assert_eq!(
            kept[0].1,
            "AWS4-HMAC-SHA256 Credential=test/20261112/us-east-1/iam/aws4_request, SignedHeaders=host, Signature=***"
        );
        assert_eq!(
            kept[1],
            ("x-amz-security-token".to_owned(), "***".to_owned())
        );
        assert_eq!(kept[2].1, "aws-cli/2");
    }

    #[test]
    fn names_what_a_request_is_about() {
        let form = b"Action=GetRole&Version=2010-05-08&RoleName=app";
        assert_eq!(resource(Protocol::Query, &[], form), "app");
        let json = br#"{"Limit":5,"TableName":"orders"}"#;
        assert_eq!(resource(Protocol::Json10, &[], json), "orders");
        assert_eq!(resource(Protocol::Json11, &[], b"not json"), "");
        assert_eq!(resource(Protocol::RestXml, &[], b""), "");
    }
}
