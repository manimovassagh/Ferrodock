//! The operations on queues: create, find, list, delete, attributes,
//! purge, tags, and the queues that send to a dead-letter queue.

use std::collections::BTreeMap;

use serde_json::{Value, json};

use super::input::Input;
use super::{Ctx, attributes, fault, queue_name};
use crate::protocol::Fault;
use crate::queues::{Settings, records};

/// 1 to 80 letters, digits, `-` and `_`, plus `.fifo` for a FIFO queue.
fn check_name(name: &str, fifo: bool) -> Result<(), Fault> {
    let base = name.strip_suffix(".fifo").unwrap_or(name);
    let ok = !base.is_empty()
        && name.len() <= 80
        && base
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        && fifo == name.ends_with(".fifo");
    if ok {
        Ok(())
    } else {
        Err(Fault::bad_request(
            "InvalidParameterValue",
            "Can only include alphanumeric characters, hyphens, or underscores. 1 to 80 in length. \
             A FIFO queue's name ends in .fifo, and only a FIFO queue's does.",
        )
        .query_code("InvalidParameterValue"))
    }
}

pub fn create(ctx: &Ctx, input: &Input) -> Result<Value, Fault> {
    let name = input.required("QueueName")?;
    let given = input.map("Attributes");
    let mut settings = Settings::default();
    let mut extra = BTreeMap::new();
    attributes::apply(&given, &mut settings, &mut extra)?;
    check_name(&name, settings.fifo)?;
    if settings.content_dedup && !settings.fifo {
        return Err(Fault::bad_request(
            "InvalidAttributeName",
            "ContentBasedDeduplication is only for FIFO queues.",
        )
        .query_code("InvalidAttributeName"));
    }
    // `tags` in JSON (lowercase, unlike TagQueue's), `Tag.N` in forms.
    let mut tags: BTreeMap<String, String> = input.map("tags").into_iter().collect();
    tags.extend(input.map("Tags"));
    records::create(ctx.store, &name, settings, extra, tags, ctx.now).map_err(fault)?;
    Ok(json!({ "QueueUrl": ctx.url(&name) }))
}

pub fn get_url(ctx: &Ctx, input: &Input) -> Result<Value, Fault> {
    let name = input.required("QueueName")?;
    records::get(ctx.store, &name).map_err(fault)?;
    Ok(json!({ "QueueUrl": ctx.url(&name) }))
}

pub fn list(ctx: &Ctx, input: &Input) -> Result<Value, Fault> {
    let prefix = input.text("QueueNamePrefix").unwrap_or_default();
    let max = input.number("MaxResults")?.unwrap_or(1000) as usize;
    let start = input.text("NextToken").unwrap_or_default();
    let names: Vec<String> = records::list(ctx.store, &prefix)
        .map_err(fault)?
        .into_iter()
        .map(|q| q.name)
        .filter(|n| *n > start)
        .collect();
    let urls: Vec<String> = names.iter().take(max).map(|n| ctx.url(n)).collect();
    let mut out = json!({ "QueueUrls": urls });
    if names.len() > max {
        out["NextToken"] = json!(names[max - 1]);
    }
    Ok(out)
}

pub fn delete(ctx: &Ctx, input: &Input) -> Result<Value, Fault> {
    records::delete(ctx.store, &queue_name(input)?).map_err(fault)?;
    Ok(json!({}))
}

pub fn purge(ctx: &Ctx, input: &Input) -> Result<Value, Fault> {
    records::purge(ctx.store, &queue_name(input)?).map_err(fault)?;
    Ok(json!({}))
}

pub fn get_attributes(ctx: &Ctx, input: &Input) -> Result<Value, Fault> {
    let name = queue_name(input)?;
    let queue = records::get(ctx.store, &name).map_err(fault)?;
    let counts = records::counts(ctx.store, &name, ctx.now).map_err(fault)?;
    let all = attributes::all(&queue, counts, &ctx.arn(&name));
    let picked = attributes::pick(all, &input.list("AttributeNames"))?;
    Ok(if picked.is_empty() {
        json!({})
    } else {
        json!({ "Attributes": picked })
    })
}

pub fn set_attributes(ctx: &Ctx, input: &Input) -> Result<Value, Fault> {
    let name = queue_name(input)?;
    let queue = records::get(ctx.store, &name).map_err(fault)?;
    let (mut settings, mut extra) = (queue.settings.clone(), queue.extra.clone());
    let given = input.map("Attributes");
    if given.iter().any(|(k, _)| k == "FifoQueue") {
        return Err(Fault::bad_request(
            "InvalidAttributeName",
            "FifoQueue can only be set when the queue is made.",
        )
        .query_code("InvalidAttributeName"));
    }
    attributes::apply(&given, &mut settings, &mut extra)?;
    records::update(ctx.store, &name, &settings, &extra, ctx.now).map_err(fault)?;
    Ok(json!({}))
}

pub fn tag(ctx: &Ctx, input: &Input) -> Result<Value, Fault> {
    let name = queue_name(input)?;
    let mut tags = records::get(ctx.store, &name).map_err(fault)?.tags;
    tags.extend(input.map("Tags"));
    records::set_tags(ctx.store, &name, &tags).map_err(fault)?;
    Ok(json!({}))
}

pub fn untag(ctx: &Ctx, input: &Input) -> Result<Value, Fault> {
    let name = queue_name(input)?;
    let mut tags = records::get(ctx.store, &name).map_err(fault)?.tags;
    for key in input.list("TagKeys") {
        tags.remove(&key);
    }
    records::set_tags(ctx.store, &name, &tags).map_err(fault)?;
    Ok(json!({}))
}

pub fn list_tags(ctx: &Ctx, input: &Input) -> Result<Value, Fault> {
    let tags = records::get(ctx.store, &queue_name(input)?)
        .map_err(fault)?
        .tags;
    Ok(if tags.is_empty() {
        json!({})
    } else {
        json!({ "Tags": tags })
    })
}

pub fn dead_letter_sources(ctx: &Ctx, input: &Input) -> Result<Value, Fault> {
    let name = queue_name(input)?;
    records::get(ctx.store, &name).map_err(fault)?;
    let urls: Vec<String> = records::sources(ctx.store, &name)
        .map_err(fault)?
        .iter()
        .map(|n| ctx.url(n))
        .collect();
    Ok(json!({ "queueUrls": urls }))
}
