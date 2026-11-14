//! SQS: message queues over the queue core in [`crate::queues`].
//!
//! Both protocols SQS is spoken in work: JSON 1.0 (current SDKs and the
//! CLI) and the query protocol (older SDKs and tools). Each operation is
//! written once, on JSON, and the reply is written in the protocol the
//! request came in. A receive with a wait holds the request until a
//! message arrives or the wait ends (long polling).
//!
//! - `input.rs`: a request's fields, the same whichever protocol.
//! - `queues.rs`: creating, finding, listing, deleting and tagging queues.
//! - `messages.rs`: sending, receiving, deleting and hiding messages.
//! - `attributes.rs`: queue attributes, and message attributes' MD5.
//! - `inventory.rs`: queues and messages, as the web console shows them.
//! - `xml.rs`: replies in the query protocol.

mod attributes;
mod input;
mod inventory;
mod messages;
mod queues;
mod xml;

use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use tokio::sync::Notify;

use crate::protocol::{Fault, Protocol, Reply};
use crate::queues::{Problem, records};
use crate::service::{Call, Handled, Listing, Service};
use crate::store::Store;
use crate::{ACCOUNT, clock};
use input::Input;

/// How often a waiting receive looks again, for delayed messages and
/// those whose visibility ends: a send wakes it at once.
const LOOK_AGAIN: Duration = Duration::from_millis(250);

#[derive(Default)]
pub struct Sqs {
    /// Woken by each send, for receives that wait.
    arrived: Arc<Notify>,
}

/// What every operation needs: the store, the time, and how to name a
/// queue to the caller.
pub struct Ctx<'a> {
    store: &'a Store,
    now: u64,
    /// The host the request was sent to, so queue URLs work from where
    /// the caller is (the computer, or another container).
    host: String,
    region: String,
}

impl Ctx<'_> {
    fn url(&self, name: &str) -> String {
        format!("http://{}/{ACCOUNT}/{name}", self.host)
    }

    fn arn(&self, name: &str) -> String {
        format!("arn:aws:sqs:{}:{ACCOUNT}:{name}", self.region)
    }
}

/// The queue a request is about: from `QueueUrl`, or the path it was sent
/// to (older SDKs send to the queue's URL).
fn queue_name(input: &Input) -> Result<String, Fault> {
    input
        .text("QueueUrl")
        .and_then(|url| {
            url.trim_end_matches('/')
                .rsplit('/')
                .next()
                .map(str::to_owned)
        })
        .filter(|name| !name.is_empty())
        .or_else(|| input.text("__path").filter(|p| !p.is_empty()))
        .ok_or_else(|| {
            Fault::bad_request(
                "MissingParameter",
                "The request must contain the parameter QueueUrl.",
            )
            .query_code("MissingParameter")
        })
}

/// The queue core's problems, worded as SQS does.
fn fault(problem: Problem) -> Fault {
    match problem {
        Problem::NoQueue(_) => Fault::bad_request(
            "QueueDoesNotExist",
            "The specified queue does not exist.",
        )
        .query_code("AWS.SimpleQueueService.NonExistentQueue"),
        Problem::QueueExists(name) => Fault::bad_request(
            "QueueNameExists",
            format!("A queue already exists with the same name ({name}) and a different value for an attribute."),
        )
        .query_code("QueueAlreadyExists"),
        Problem::BadReceipt(receipt) => Fault::bad_request(
            "ReceiptHandleIsInvalid",
            format!("The input receipt handle \"{receipt}\" is not a valid receipt handle."),
        )
        .query_code("ReceiptHandleIsInvalid"),
        Problem::NotInFlight => Fault::bad_request(
            "MessageNotInflight",
            "The message referred to isn't in flight.",
        )
        .query_code("AWS.SimpleQueueService.MessageNotInflight"),
        Problem::Internal(e) => Fault::internal(e),
    }
}

/// The reply, in the protocol the request came in.
fn reply(call: &Call, action: &str, result: &Value) -> Reply {
    match call.route.protocol {
        Protocol::Query => xml::reply(action, result, &call.request_id),
        _ => Reply::ok(Protocol::Json10, result.to_string()),
    }
}

impl Sqs {
    /// Receives, waiting up to the asked time for a message to come.
    async fn receive(&self, call: &Call, store: &Store, input: &Input) -> Result<Value, Fault> {
        let (name, ask) = {
            let ctx = context(call, store);
            messages::ask(&ctx, input)?
        };
        let deadline = tokio::time::Instant::now() + Duration::from_secs(ask.wait);
        loop {
            // Asked for before looking, so a send in between still wakes it.
            let arrived = self.arrived.notified();
            let got = records::receive(store, &name, ask.max, ask.visibility, clock::now_millis())
                .map_err(fault)?;
            let now = tokio::time::Instant::now();
            if !got.is_empty() || now >= deadline {
                return messages::received(input, &got);
            }
            let pause = LOOK_AGAIN.min(deadline - now);
            let _ = tokio::time::timeout(pause, arrived).await;
        }
    }
}

fn context<'a>(call: &Call, store: &'a Store) -> Ctx<'a> {
    let host = call
        .headers
        .get("host")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("localhost:4566")
        .to_owned();
    Ctx {
        store,
        now: clock::now_millis(),
        host,
        region: call.route.region.clone(),
    }
}

/// Runs an operation that doesn't wait.
fn run(action: &str, ctx: &Ctx, input: &Input) -> Result<Value, Fault> {
    match action {
        "CreateQueue" => queues::create(ctx, input),
        "GetQueueUrl" => queues::get_url(ctx, input),
        "ListQueues" => queues::list(ctx, input),
        "DeleteQueue" => queues::delete(ctx, input),
        "PurgeQueue" => queues::purge(ctx, input),
        "GetQueueAttributes" => queues::get_attributes(ctx, input),
        "SetQueueAttributes" => queues::set_attributes(ctx, input),
        "TagQueue" => queues::tag(ctx, input),
        "UntagQueue" => queues::untag(ctx, input),
        "ListQueueTags" => queues::list_tags(ctx, input),
        "ListDeadLetterSourceQueues" => queues::dead_letter_sources(ctx, input),
        "SendMessage" => messages::send(ctx, input),
        "SendMessageBatch" => messages::send_batch(ctx, input),
        "DeleteMessage" => messages::delete(ctx, input),
        "DeleteMessageBatch" => messages::delete_batch(ctx, input),
        "ChangeMessageVisibility" => messages::change_visibility(ctx, input),
        "ChangeMessageVisibilityBatch" => messages::change_visibility_batch(ctx, input),
        other => Err(Fault::bad_request(
            "InvalidAction",
            format!(
                "ferrocloud's SQS doesn't support {other} yet. What it does: /_ferrocloud/services"
            ),
        )
        .query_code("InvalidAction")),
    }
}

impl Service for Sqs {
    fn name(&self) -> &'static str {
        "sqs"
    }

    fn title(&self) -> &'static str {
        "SQS"
    }

    fn signing_names(&self) -> &'static [&'static str] {
        &["sqs"]
    }

    fn target_prefixes(&self) -> &'static [&'static str] {
        &["AmazonSQS"]
    }

    fn operations(&self) -> &'static [&'static str] {
        OPERATIONS
    }

    fn init(&self, store: &Store) -> Result<(), String> {
        records::init(store)
    }

    fn handle<'a>(&'a self, call: Call, store: &'a Store) -> Handled<'a> {
        Box::pin(async move {
            let mut input = Input::of(&call)?;
            // A request sent to a queue's URL names the queue in its path.
            let path_name = call
                .path
                .trim_end_matches('/')
                .rsplit('/')
                .next()
                .unwrap_or_default();
            if call.path.matches('/').count() >= 2 {
                input.set_default("__path", path_name);
            }
            let action = call.route.operation.clone().unwrap_or_default();
            let result = if action == "ReceiveMessage" {
                self.receive(&call, store, &input).await?
            } else {
                let result = run(&action, &context(&call, store), &input)?;
                if action.starts_with("SendMessage") {
                    self.arrived.notify_waiters();
                }
                result
            };
            Ok(reply(&call, &action, &result))
        })
    }

    fn inventory(&self, store: &Store) -> Result<Vec<Listing>, String> {
        inventory::inventory(store)
    }

    fn browse(&self, store: &Store, path: &str) -> Result<Value, String> {
        inventory::browse(store, path)
    }
}

const OPERATIONS: &[&str] = &[
    "ChangeMessageVisibility",
    "ChangeMessageVisibilityBatch",
    "CreateQueue",
    "DeleteMessage",
    "DeleteMessageBatch",
    "DeleteQueue",
    "GetQueueAttributes",
    "GetQueueUrl",
    "ListDeadLetterSourceQueues",
    "ListQueueTags",
    "ListQueues",
    "PurgeQueue",
    "ReceiveMessage",
    "SendMessage",
    "SendMessageBatch",
    "SetQueueAttributes",
    "TagQueue",
    "UntagQueue",
];
