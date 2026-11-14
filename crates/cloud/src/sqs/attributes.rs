//! Queue attributes, between SQS's names and the queue core's settings,
//! and message attributes with the MD5 the SDKs check them against.

use std::collections::BTreeMap;

use serde_json::{Map, Value, json};

use crate::md5::md5_hex;
use crate::protocol::Fault;
use crate::queues::{Counts, Queue, Settings, records::Redrive};

/// Attributes a queue can be given and keeps as they are.
const KEPT: &[&str] = &[
    "Policy",
    "RedriveAllowPolicy",
    "KmsMasterKeyId",
    "KmsDataKeyReusePeriodSeconds",
    "SqsManagedSseEnabled",
    "DeduplicationScope",
    "FifoThroughputLimit",
];

/// Attributes only read, never set.
const READ_ONLY: &[&str] = &[
    "QueueArn",
    "ApproximateNumberOfMessages",
    "ApproximateNumberOfMessagesNotVisible",
    "ApproximateNumberOfMessagesDelayed",
    "CreatedTimestamp",
    "LastModifiedTimestamp",
];

fn bad_name(name: &str) -> Fault {
    Fault::bad_request("InvalidAttributeName", format!("Unknown Attribute {name}."))
        .query_code("InvalidAttributeName")
}

fn bad_value(name: &str, value: &str) -> Fault {
    Fault::bad_request(
        "InvalidAttributeValue",
        format!("Invalid value for the parameter {name}: {value}"),
    )
    .query_code("InvalidAttributeValue")
}

fn seconds(name: &str, value: &str, max: u64) -> Result<u64, Fault> {
    value
        .parse::<u64>()
        .ok()
        .filter(|n| *n <= max)
        .ok_or_else(|| bad_value(name, value))
}

/// Applies `attributes` to `settings` and `extra`, checking each.
pub fn apply(
    attributes: &[(String, String)],
    settings: &mut Settings,
    extra: &mut BTreeMap<String, String>,
) -> Result<(), Fault> {
    for (name, value) in attributes {
        match name.as_str() {
            "VisibilityTimeout" => settings.visibility_timeout = seconds(name, value, 43_200)?,
            "DelaySeconds" => settings.delay = seconds(name, value, 900)?,
            "MessageRetentionPeriod" => settings.retention = seconds(name, value, 1_209_600)?,
            "ReceiveMessageWaitTimeSeconds" => settings.wait = seconds(name, value, 20)?,
            "MaximumMessageSize" => settings.max_size = seconds(name, value, 262_144)?,
            "FifoQueue" => settings.fifo = value == "true",
            "ContentBasedDeduplication" => settings.content_dedup = value == "true",
            "RedrivePolicy" if value.is_empty() => settings.redrive = None,
            "RedrivePolicy" => settings.redrive = Some(redrive(value)?),
            name if KEPT.contains(&name) => {
                extra.insert(name.to_owned(), value.clone());
            }
            name => return Err(bad_name(name)),
        }
    }
    Ok(())
}

/// `{"deadLetterTargetArn": "arn:…:dlq", "maxReceiveCount": 3}`.
fn redrive(value: &str) -> Result<Redrive, Fault> {
    let policy: Value =
        serde_json::from_str(value).map_err(|_| bad_value("RedrivePolicy", value))?;
    let target = policy["deadLetterTargetArn"]
        .as_str()
        .and_then(|arn| arn.rsplit(':').next())
        .filter(|name| !name.is_empty());
    let count = match &policy["maxReceiveCount"] {
        Value::Number(n) => n.as_u64(),
        Value::String(s) => s.parse().ok(),
        _ => None,
    };
    match (target, count) {
        (Some(target), Some(max_receives)) if max_receives >= 1 => Ok(Redrive {
            target: target.to_owned(),
            max_receives: max_receives as u32,
        }),
        _ => Err(bad_value("RedrivePolicy", value)),
    }
}

/// Every attribute of a queue, by name.
pub fn all(queue: &Queue, counts: Counts, arn: &str) -> BTreeMap<String, String> {
    let s = &queue.settings;
    let mut out = BTreeMap::from([
        ("QueueArn".to_owned(), arn.to_owned()),
        (
            "ApproximateNumberOfMessages".to_owned(),
            counts.visible.to_string(),
        ),
        (
            "ApproximateNumberOfMessagesNotVisible".to_owned(),
            counts.in_flight.to_string(),
        ),
        (
            "ApproximateNumberOfMessagesDelayed".to_owned(),
            counts.delayed.to_string(),
        ),
        (
            "CreatedTimestamp".to_owned(),
            (queue.created / 1000).to_string(),
        ),
        (
            "LastModifiedTimestamp".to_owned(),
            (queue.modified / 1000).to_string(),
        ),
        (
            "VisibilityTimeout".to_owned(),
            s.visibility_timeout.to_string(),
        ),
        ("MaximumMessageSize".to_owned(), s.max_size.to_string()),
        ("MessageRetentionPeriod".to_owned(), s.retention.to_string()),
        ("DelaySeconds".to_owned(), s.delay.to_string()),
        (
            "ReceiveMessageWaitTimeSeconds".to_owned(),
            s.wait.to_string(),
        ),
        ("SqsManagedSseEnabled".to_owned(), "false".to_owned()),
    ]);
    if s.fifo {
        out.insert("FifoQueue".into(), "true".into());
        out.insert(
            "ContentBasedDeduplication".into(),
            s.content_dedup.to_string(),
        );
    }
    if let Some(redrive) = &s.redrive {
        let target = arn.rsplit_once(':').map_or(arn, |(prefix, _)| prefix);
        out.insert(
            "RedrivePolicy".into(),
            json!({
                "deadLetterTargetArn": format!("{target}:{}", redrive.target),
                "maxReceiveCount": redrive.max_receives,
            })
            .to_string(),
        );
    }
    out.extend(queue.extra.clone());
    out
}

/// The attributes asked for (`All`, or names), checking the names.
pub fn pick(all: BTreeMap<String, String>, names: &[String]) -> Result<Map<String, Value>, Fault> {
    let known = |n: &str| {
        all.contains_key(n)
            || KEPT.contains(&n)
            || READ_ONLY.contains(&n)
            || matches!(
                n,
                "FifoQueue" | "ContentBasedDeduplication" | "RedrivePolicy"
            )
    };
    if let Some(bad) = names.iter().find(|n| *n != "All" && !known(n)) {
        return Err(bad_name(bad));
    }
    let every = names.iter().any(|n| n == "All");
    Ok(all
        .into_iter()
        .filter(|(k, _)| every || names.contains(k))
        .map(|(k, v)| (k, Value::String(v)))
        .collect())
}

// Message attributes.

/// Checks message attributes (each needs a data type, and the value it
/// says) and gives back their MD5, or None when there are none.
pub fn message_md5(attributes: &Value) -> Result<Option<String>, Fault> {
    let Some(map) = attributes.as_object().filter(|m| !m.is_empty()) else {
        return Ok(None);
    };
    let mut names: Vec<&String> = map.keys().collect();
    names.sort();
    let mut bytes = Vec::new();
    /// A part with its length first, as the checksum takes it.
    fn push(bytes: &mut Vec<u8>, part: &[u8]) {
        bytes.extend_from_slice(&(part.len() as u32).to_be_bytes());
        bytes.extend_from_slice(part);
    }
    for name in names {
        let value = &map[name];
        let kind = value["DataType"].as_str().unwrap_or_default();
        if kind.is_empty() {
            return Err(bad_value(
                &format!("MessageAttribute.{name}"),
                "missing DataType",
            ));
        }
        push(&mut bytes, name.as_bytes());
        push(&mut bytes, kind.as_bytes());
        if kind.starts_with("Binary") {
            let raw = value["BinaryValue"]
                .as_str()
                .and_then(base64_decode)
                .ok_or_else(|| bad_value(&format!("MessageAttribute.{name}"), "BinaryValue"))?;
            bytes.push(2);
            push(&mut bytes, &raw);
        } else {
            let text = value["StringValue"]
                .as_str()
                .ok_or_else(|| bad_value(&format!("MessageAttribute.{name}"), "StringValue"))?;
            bytes.push(1);
            push(&mut bytes, text.as_bytes());
        }
    }
    Ok(Some(md5_hex(&bytes)))
}

/// The message attributes asked for by name (`All` or `.*` for every one,
/// `prefix.*` for those under a prefix).
pub fn pick_message(attributes: &Value, names: &[String]) -> Map<String, Value> {
    let Some(map) = attributes.as_object() else {
        return Map::new();
    };
    let wanted = |name: &str| {
        names.iter().any(|n| {
            n == "All"
                || n == ".*"
                || n == name
                || n.strip_suffix(".*")
                    .is_some_and(|p| name.starts_with(&format!("{p}.")))
        })
    };
    map.iter()
        .filter(|(k, _)| wanted(k))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

/// Standard base64, with or without padding.
fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let value = |c: u8| match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    };
    let clean: Vec<u8> = text
        .bytes()
        .filter(|c| !c.is_ascii_whitespace() && *c != b'=')
        .collect();
    let mut out = Vec::with_capacity(clean.len() * 3 / 4);
    for chunk in clean.chunks(4) {
        let mut acc = 0u32;
        for (i, c) in chunk.iter().enumerate() {
            acc |= u32::from(value(*c)?) << (18 - 6 * i);
        }
        let bytes = acc.to_be_bytes();
        out.extend_from_slice(&bytes[1..chunk.len()]);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn md5_of_message_attributes_matches_aws() {
        // The bytes as SQS's documentation lays them out: each name, type
        // and value with its length first, and 1 before a text value.
        let attributes = json!({"kind": {"DataType": "String", "StringValue": "order"}});
        let md5 = message_md5(&attributes).unwrap().unwrap();
        let mut bytes = Vec::new();
        for part in [&b"kind"[..], b"String"] {
            bytes.extend_from_slice(&(part.len() as u32).to_be_bytes());
            bytes.extend_from_slice(part);
        }
        bytes.push(1);
        bytes.extend_from_slice(&5u32.to_be_bytes());
        bytes.extend_from_slice(b"order");
        assert_eq!(md5, md5_hex(&bytes));
        assert_eq!(message_md5(&json!({})).unwrap(), None);
        assert!(message_md5(&json!({"x": {"StringValue": "1"}})).is_err());
    }

    #[test]
    fn decodes_base64() {
        assert_eq!(base64_decode("aGVsbG8=").unwrap(), b"hello");
        assert_eq!(base64_decode("aGk").unwrap(), b"hi");
        assert!(base64_decode("!!").is_none());
    }

    #[test]
    fn reads_and_checks_queue_attributes() {
        let mut settings = Settings::default();
        let mut extra = BTreeMap::new();
        let attributes = [
            ("DelaySeconds".to_owned(), "5".to_owned()),
            (
                "RedrivePolicy".to_owned(),
                r#"{"deadLetterTargetArn":"arn:aws:sqs:us-east-1:000000000000:dlq","maxReceiveCount":"3"}"#.to_owned(),
            ),
            ("Policy".to_owned(), "{}".to_owned()),
        ];
        apply(&attributes, &mut settings, &mut extra).unwrap();
        assert_eq!(settings.delay, 5);
        assert_eq!(settings.redrive.as_ref().unwrap().target, "dlq");
        assert_eq!(extra["Policy"], "{}");
        assert!(apply(&[("Nope".into(), "1".into())], &mut settings, &mut extra).is_err());
        assert!(
            apply(
                &[("DelaySeconds".into(), "901".into())],
                &mut settings,
                &mut extra
            )
            .is_err()
        );
    }
}
