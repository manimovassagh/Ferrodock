//! The operations on messages: send, receive, delete and change their
//! visibility, one at a time or in batches of up to ten.

use std::collections::HashSet;

use serde_json::{Map, Value, json};

use super::input::{Input, invalid};
use super::{Ctx, attributes, fault, queue_name};
use crate::md5::md5_hex;
use crate::protocol::Fault;
use crate::queues::{NewMessage, Queue, Received, records};

/// The most messages one receive or batch takes.
const MOST: usize = 10;

fn batch_fault(code: &str, message: &str) -> Fault {
    Fault::bad_request(code, message).query_code(&format!("AWS.SimpleQueueService.{code}"))
}

/// Checks a batch's entries: 1 to 10, with distinct IDs.
fn check_batch(entries: &[Input]) -> Result<(), Fault> {
    if entries.is_empty() {
        return Err(batch_fault(
            "EmptyBatchRequest",
            "There should be at least one entry in the request.",
        ));
    }
    if entries.len() > MOST {
        return Err(batch_fault(
            "TooManyEntriesInBatchRequest",
            &format!(
                "Maximum number of entries per request are {MOST}. You have sent {}.",
                entries.len()
            ),
        ));
    }
    let mut seen = HashSet::new();
    for entry in entries {
        if !seen.insert(entry.text("Id").unwrap_or_default()) {
            return Err(batch_fault(
                "BatchEntryIdsNotDistinct",
                "Two or more batch entries have the same Id.",
            ));
        }
    }
    Ok(())
}

/// One entry that failed, as a batch reports it.
fn failed(id: &str, fault: &Fault) -> Value {
    json!({
        "Id": id,
        "SenderFault": fault.status < 500,
        "Code": fault.query_code.as_deref().unwrap_or(&fault.code),
        "Message": fault.message,
    })
}

fn send_one(ctx: &Ctx, queue: &Queue, input: &Input) -> Result<Value, Fault> {
    let body = input.required("MessageBody")?;
    if body.len() as u64 > queue.settings.max_size {
        return Err(invalid("MessageBody", "too long"));
    }
    let attributes = input
        .value("MessageAttributes")
        .cloned()
        .unwrap_or(Value::Null);
    let attributes_md5 = attributes::message_md5(&attributes)?;
    let group = input.text("MessageGroupId");
    let dedup = input.text("MessageDeduplicationId");
    if queue.settings.fifo {
        if group.is_none() {
            return Err(Fault::bad_request(
                "MissingParameter",
                "The request must contain the parameter MessageGroupId.",
            )
            .query_code("MissingParameter"));
        }
        if dedup.is_none() && !queue.settings.content_dedup {
            return Err(invalid(
                "MessageDeduplicationId",
                "missing: the queue should either have ContentBasedDeduplication enabled or \
                 MessageDeduplicationId provided explicitly",
            ));
        }
    }
    let delay = input.number("DelaySeconds")?;
    if delay.is_some_and(|d| d > 900) {
        return Err(invalid(
            "DelaySeconds",
            &delay.unwrap_or_default().to_string(),
        ));
    }
    let md5 = md5_hex(body.as_bytes());
    let message = NewMessage {
        body,
        attributes,
        delay,
        group,
        dedup,
    };
    let sent = records::send(ctx.store, &queue.name, message, ctx.now).map_err(fault)?;
    let mut out = json!({ "MessageId": sent.id, "MD5OfMessageBody": md5 });
    if let Some(md5) = attributes_md5 {
        out["MD5OfMessageAttributes"] = json!(md5);
    }
    if queue.settings.fifo {
        out["SequenceNumber"] = json!(format!("{:020}", sent.sequence));
    }
    Ok(out)
}

pub fn send(ctx: &Ctx, input: &Input) -> Result<Value, Fault> {
    let queue = records::get(ctx.store, &queue_name(input)?).map_err(fault)?;
    send_one(ctx, &queue, input)
}

pub fn send_batch(ctx: &Ctx, input: &Input) -> Result<Value, Fault> {
    let queue = records::get(ctx.store, &queue_name(input)?).map_err(fault)?;
    let entries = input.entries("Entries");
    check_batch(&entries)?;
    let (mut ok, mut bad) = (Vec::new(), Vec::new());
    for entry in &entries {
        let id = entry.text("Id").unwrap_or_default();
        match send_one(ctx, &queue, entry) {
            Ok(mut sent) => {
                sent["Id"] = json!(id);
                ok.push(sent);
            }
            Err(fault) => bad.push(failed(&id, &fault)),
        }
    }
    Ok(json!({ "Successful": ok, "Failed": bad }))
}

/// How many messages a receive asks for, and for how long they hide.
pub struct Ask {
    pub max: usize,
    pub visibility: u64,
    /// Seconds to wait for a message when there's none.
    pub wait: u64,
}

pub fn ask(ctx: &Ctx, input: &Input) -> Result<(String, Ask), Fault> {
    let name = queue_name(input)?;
    let queue = records::get(ctx.store, &name).map_err(fault)?;
    let max = input.number("MaxNumberOfMessages")?.unwrap_or(1);
    if !(1..=MOST as u64).contains(&max) {
        return Err(invalid("MaxNumberOfMessages", &max.to_string()));
    }
    let wait = input
        .number("WaitTimeSeconds")?
        .unwrap_or(queue.settings.wait);
    if wait > 20 {
        return Err(invalid("WaitTimeSeconds", &wait.to_string()));
    }
    let visibility = input
        .number("VisibilityTimeout")?
        .unwrap_or(queue.settings.visibility_timeout);
    Ok((
        name,
        Ask {
            max: max as usize,
            visibility,
            wait,
        },
    ))
}

/// The system attributes of a received message, by name.
fn system_attributes(received: &Received) -> Map<String, Value> {
    let m = &received.message;
    let mut out = Map::new();
    out.insert("SenderId".into(), json!(crate::ACCOUNT));
    out.insert("SentTimestamp".into(), json!(m.sent.to_string()));
    out.insert(
        "ApproximateReceiveCount".into(),
        json!(m.receives.to_string()),
    );
    out.insert(
        "ApproximateFirstReceiveTimestamp".into(),
        json!(m.first_received.unwrap_or(m.sent).to_string()),
    );
    if let Some(group) = &m.group {
        out.insert("MessageGroupId".into(), json!(group));
        out.insert(
            "SequenceNumber".into(),
            json!(format!("{:020}", m.sequence)),
        );
    }
    if let Some(dedup) = &m.dedup {
        out.insert("MessageDeduplicationId".into(), json!(dedup));
    }
    out
}

/// Received messages as the reply lists them, with the attributes asked for.
pub fn received(input: &Input, messages: &[Received]) -> Result<Value, Fault> {
    let mut wanted = input.list("AttributeNames");
    wanted.extend(input.list("MessageSystemAttributeNames"));
    let every = wanted.iter().any(|n| n == "All");
    let attribute_names = input.list("MessageAttributeNames");
    let mut out = Vec::new();
    for r in messages {
        let mut message = json!({
            "MessageId": r.message.id,
            "ReceiptHandle": r.receipt,
            "MD5OfBody": md5_hex(r.message.body.as_bytes()),
            "Body": r.message.body,
        });
        let system: Map<String, Value> = system_attributes(r)
            .into_iter()
            .filter(|(k, _)| every || wanted.contains(k))
            .collect();
        if !system.is_empty() {
            message["Attributes"] = Value::Object(system);
        }
        let attributes = attributes::pick_message(&r.message.attributes, &attribute_names);
        if !attributes.is_empty() {
            let value = Value::Object(attributes);
            message["MD5OfMessageAttributes"] = json!(attributes::message_md5(&value)?);
            message["MessageAttributes"] = value;
        }
        out.push(message);
    }
    Ok(if out.is_empty() {
        json!({})
    } else {
        json!({ "Messages": out })
    })
}

pub fn delete(ctx: &Ctx, input: &Input) -> Result<Value, Fault> {
    let name = queue_name(input)?;
    let receipt = input.required("ReceiptHandle")?;
    records::delete_message(ctx.store, &name, &receipt).map_err(fault)?;
    Ok(json!({}))
}

pub fn delete_batch(ctx: &Ctx, input: &Input) -> Result<Value, Fault> {
    let name = queue_name(input)?;
    records::get(ctx.store, &name).map_err(fault)?;
    let entries = input.entries("Entries");
    check_batch(&entries)?;
    let (mut ok, mut bad) = (Vec::new(), Vec::new());
    for entry in &entries {
        let id = entry.text("Id").unwrap_or_default();
        let done = entry
            .required("ReceiptHandle")
            .and_then(|receipt| records::delete_message(ctx.store, &name, &receipt).map_err(fault));
        match done {
            Ok(()) => ok.push(json!({ "Id": id })),
            Err(fault) => bad.push(failed(&id, &fault)),
        }
    }
    Ok(json!({ "Successful": ok, "Failed": bad }))
}

fn change_one(ctx: &Ctx, name: &str, input: &Input) -> Result<(), Fault> {
    let receipt = input.required("ReceiptHandle")?;
    let seconds = input.number("VisibilityTimeout")?.unwrap_or(0);
    if seconds > 43_200 {
        return Err(invalid("VisibilityTimeout", &seconds.to_string()));
    }
    records::change_visibility(ctx.store, name, &receipt, seconds, ctx.now).map_err(fault)
}

pub fn change_visibility(ctx: &Ctx, input: &Input) -> Result<Value, Fault> {
    let name = queue_name(input)?;
    change_one(ctx, &name, input)?;
    Ok(json!({}))
}

pub fn change_visibility_batch(ctx: &Ctx, input: &Input) -> Result<Value, Fault> {
    let name = queue_name(input)?;
    records::get(ctx.store, &name).map_err(fault)?;
    let entries = input.entries("Entries");
    check_batch(&entries)?;
    let (mut ok, mut bad) = (Vec::new(), Vec::new());
    for entry in &entries {
        let id = entry.text("Id").unwrap_or_default();
        match change_one(ctx, &name, entry) {
            Ok(()) => ok.push(json!({ "Id": id })),
            Err(fault) => bad.push(failed(&id, &fault)),
        }
    }
    Ok(json!({ "Successful": ok, "Failed": bad }))
}
