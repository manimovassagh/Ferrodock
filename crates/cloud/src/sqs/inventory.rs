//! Queues as the web console lists them, with their message counts, and
//! a queue's messages looked at without being received.

use serde_json::{Value, json};

use crate::clock;
use crate::queues::records;
use crate::service::{Item, Listing};
use crate::store::Store;

/// How many messages the console shows of a queue.
const SHOWN: usize = 50;

pub fn inventory(store: &Store) -> Result<Vec<Listing>, String> {
    let now = clock::now_millis();
    let mut items = Vec::new();
    for queue in records::list(store, "").map_err(|e| format!("{e:?}"))? {
        let counts = records::counts(store, &queue.name, now).map_err(|e| format!("{e:?}"))?;
        let kind = if queue.settings.fifo {
            "FIFO"
        } else {
            "Standard"
        };
        items.push(Item {
            cells: vec![
                queue.name.clone(),
                kind.to_owned(),
                counts.visible.to_string(),
                counts.in_flight.to_string(),
                counts.delayed.to_string(),
                clock::iso8601_millis(queue.created),
            ],
            detail: json!({
                "name": queue.name,
                "fifo": queue.settings.fifo,
                "counts": counts,
                "settings": queue.settings,
                "created": clock::iso8601_millis(queue.created),
                "tags": queue.tags,
            }),
        });
    }
    Ok(vec![Listing {
        title: "Queues",
        columns: &[
            "Name",
            "Type",
            "Available",
            "In flight",
            "Delayed",
            "Created",
        ],
        items,
    }])
}

/// `<queue>/messages`: the first messages in the queue, as they are.
pub fn browse(store: &Store, path: &str) -> Result<Value, String> {
    let (queue, what) = path.split_once('/').unwrap_or((path, ""));
    if what != "messages" {
        return Err(format!("nothing at {path}: try <queue>/messages"));
    }
    let now = clock::now_millis();
    let messages = records::peek(store, queue, SHOWN).map_err(|e| format!("{e:?}"))?;
    let shown: Vec<Value> = messages
        .iter()
        .map(|m| {
            let state = if m.visible_at <= now {
                "available"
            } else if m.receives > 0 {
                "in flight"
            } else {
                "delayed"
            };
            json!({
                "id": m.id,
                "body": m.body,
                "attributes": m.attributes,
                "group": m.group,
                "sent": clock::iso8601_millis(m.sent),
                "receives": m.receives,
                "state": state,
            })
        })
        .collect();
    Ok(json!({ "queue": queue, "messages": shown }))
}
