//! Queues and their messages, in SQLite. Times are milliseconds since
//! 1970, passed in, so the rules can be tested without waiting.
//!
//! A message is visible when `visible_at` has passed. Receiving it hides
//! it until the visibility timeout ends and gives it a new receipt; only
//! that receipt deletes it. One never received (`receives = 0`) that isn't
//! visible yet is delayed.

use std::collections::BTreeMap;

use rusqlite::{OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::md5::md5_hex;
use crate::store::Store;
use crate::sts::random;

/// How long a deduplication ID is remembered.
const DEDUP_WINDOW_MS: u64 = 5 * 60 * 1000;

/// What can go wrong, for each way in to word as its API does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    NoQueue(String),
    /// A queue of that name exists with other settings.
    QueueExists(String),
    /// No message in the queue has that receipt.
    BadReceipt(String),
    /// The message isn't hidden, so its visibility can't change.
    NotInFlight,
    Internal(String),
}

impl From<rusqlite::Error> for Problem {
    fn from(e: rusqlite::Error) -> Self {
        Problem::Internal(e.to_string())
    }
}

/// Where messages go after too many receives.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Redrive {
    /// The dead-letter queue's name.
    pub target: String,
    pub max_receives: u32,
}

/// How a queue behaves. Times are in seconds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub visibility_timeout: u64,
    pub delay: u64,
    pub retention: u64,
    /// How long a receive waits for a message when it doesn't say.
    pub wait: u64,
    pub max_size: u64,
    /// First in, first out, within each message group.
    pub fifo: bool,
    /// Deduplicate by the body's hash when no ID is given (FIFO only).
    pub content_dedup: bool,
    pub redrive: Option<Redrive>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            visibility_timeout: 30,
            delay: 0,
            retention: 4 * 24 * 3600,
            wait: 0,
            max_size: 256 * 1024,
            fifo: false,
            content_dedup: false,
            redrive: None,
        }
    }
}

/// A queue, with whatever else its API keeps for it (a policy, say).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Queue {
    pub name: String,
    pub settings: Settings,
    /// Kept as given and given back, by name.
    pub extra: BTreeMap<String, String>,
    pub tags: BTreeMap<String, String>,
    pub created: u64,
    pub modified: u64,
}

/// A message to send.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NewMessage {
    pub body: String,
    /// The caller's attributes, as its API encodes them; kept as is.
    pub attributes: Value,
    /// Overrides the queue's delay.
    pub delay: Option<u64>,
    pub group: Option<String>,
    pub dedup: Option<String>,
}

/// A message as kept.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StoredMessage {
    pub id: String,
    pub body: String,
    pub attributes: Value,
    pub group: Option<String>,
    pub dedup: Option<String>,
    /// Order within the queue; FIFO queues give it out.
    pub sequence: u64,
    pub sent: u64,
    pub receives: u32,
    pub first_received: Option<u64>,
    pub visible_at: u64,
}

/// A message handed to a consumer, with the receipt that deletes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Received {
    pub message: StoredMessage,
    pub receipt: String,
}

/// How many messages a queue holds, by state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Counts {
    pub visible: u64,
    pub in_flight: u64,
    pub delayed: u64,
}

pub fn init(store: &Store) -> Result<(), String> {
    store
        .db()
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS queues (
                name     TEXT PRIMARY KEY,
                settings TEXT NOT NULL,
                extra    TEXT NOT NULL,
                tags     TEXT NOT NULL,
                created  INTEGER NOT NULL,
                modified INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS queue_messages (
                id             TEXT PRIMARY KEY,
                queue          TEXT NOT NULL,
                seq            INTEGER NOT NULL,
                body           TEXT NOT NULL,
                attributes     TEXT NOT NULL,
                grp            TEXT,
                dedup          TEXT,
                sent           INTEGER NOT NULL,
                visible_at     INTEGER NOT NULL,
                receives       INTEGER NOT NULL DEFAULT 0,
                first_received INTEGER,
                receipt        TEXT
            );
            CREATE INDEX IF NOT EXISTS queue_messages_order ON queue_messages (queue, seq);
            CREATE TABLE IF NOT EXISTS queue_dedup (
                queue   TEXT NOT NULL,
                id      TEXT NOT NULL,
                until   INTEGER NOT NULL,
                message TEXT NOT NULL,
                seq     INTEGER NOT NULL,
                PRIMARY KEY (queue, id)
            );",
        )
        .map_err(|e| e.to_string())
}

fn json<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_default()
}

fn queue_row(row: &Row) -> rusqlite::Result<Queue> {
    let parse = |i: usize| -> rusqlite::Result<String> { row.get(i) };
    Ok(Queue {
        name: row.get(0)?,
        settings: serde_json::from_str(&parse(1)?).unwrap_or_default(),
        extra: serde_json::from_str(&parse(2)?).unwrap_or_default(),
        tags: serde_json::from_str(&parse(3)?).unwrap_or_default(),
        created: row.get(4)?,
        modified: row.get(5)?,
    })
}

const QUEUE: &str = "SELECT name, settings, extra, tags, created, modified FROM queues";

/// Makes a queue. One of the same name and settings is fine and given
/// back; with other settings it's an error.
pub fn create(
    store: &Store,
    name: &str,
    settings: Settings,
    extra: BTreeMap<String, String>,
    tags: BTreeMap<String, String>,
    now: u64,
) -> Result<Queue, Problem> {
    if let Some(existing) = find(store, name)? {
        if existing.settings == settings && existing.extra == extra {
            return Ok(existing);
        }
        return Err(Problem::QueueExists(name.to_owned()));
    }
    store.db().execute(
        "INSERT INTO queues (name, settings, extra, tags, created, modified) VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
        params![name, json(&settings), json(&extra), json(&tags), now],
    )?;
    get(store, name)
}

fn find(store: &Store, name: &str) -> Result<Option<Queue>, Problem> {
    Ok(store
        .db()
        .query_row(&format!("{QUEUE} WHERE name = ?1"), [name], queue_row)
        .optional()?)
}

pub fn get(store: &Store, name: &str) -> Result<Queue, Problem> {
    find(store, name)?.ok_or_else(|| Problem::NoQueue(name.to_owned()))
}

/// The queues whose names start with `prefix`, by name.
pub fn list(store: &Store, prefix: &str) -> Result<Vec<Queue>, Problem> {
    let db = store.db();
    let mut query = db.prepare(&format!("{QUEUE} ORDER BY name"))?;
    let queues = query
        .query_map([], queue_row)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(queues
        .into_iter()
        .filter(|q| q.name.starts_with(prefix))
        .collect())
}

/// Deletes a queue and its messages.
pub fn delete(store: &Store, name: &str) -> Result<(), Problem> {
    get(store, name)?;
    let db = store.db();
    db.execute("DELETE FROM queue_messages WHERE queue = ?1", [name])?;
    db.execute("DELETE FROM queue_dedup WHERE queue = ?1", [name])?;
    db.execute("DELETE FROM queues WHERE name = ?1", [name])?;
    Ok(())
}

/// Replaces a queue's settings and the rest of what's kept with it.
pub fn update(
    store: &Store,
    name: &str,
    settings: &Settings,
    extra: &BTreeMap<String, String>,
    now: u64,
) -> Result<(), Problem> {
    get(store, name)?;
    store.db().execute(
        "UPDATE queues SET settings = ?2, extra = ?3, modified = ?4 WHERE name = ?1",
        params![name, json(settings), json(extra), now],
    )?;
    Ok(())
}

pub fn set_tags(store: &Store, name: &str, tags: &BTreeMap<String, String>) -> Result<(), Problem> {
    get(store, name)?;
    store.db().execute(
        "UPDATE queues SET tags = ?2 WHERE name = ?1",
        params![name, json(tags)],
    )?;
    Ok(())
}

/// The queues that send their dead letters to `target`.
pub fn sources(store: &Store, target: &str) -> Result<Vec<String>, Problem> {
    Ok(list(store, "")?
        .into_iter()
        .filter(|q| {
            q.settings
                .redrive
                .as_ref()
                .is_some_and(|r| r.target == target)
        })
        .map(|q| q.name)
        .collect())
}

/// Deletes every message in a queue.
pub fn purge(store: &Store, name: &str) -> Result<(), Problem> {
    get(store, name)?;
    store
        .db()
        .execute("DELETE FROM queue_messages WHERE queue = ?1", [name])?;
    Ok(())
}

fn next_seq(store: &Store, queue: &str) -> Result<u64, Problem> {
    let last: Option<u64> = store.db().query_row(
        "SELECT MAX(seq) FROM queue_messages WHERE queue = ?1",
        [queue],
        |row| row.get(0),
    )?;
    let dedup: Option<u64> = store.db().query_row(
        "SELECT MAX(seq) FROM queue_dedup WHERE queue = ?1",
        [queue],
        |row| row.get(0),
    )?;
    Ok(last.max(dedup).unwrap_or(0) + 1)
}

/// What a send gave the message: its ID and place, and whether it was a
/// duplicate of one sent earlier (then those are the earlier one's).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sent {
    pub id: String,
    pub sequence: u64,
    pub duplicate: bool,
}

/// Adds a message. In a FIFO queue, a deduplication ID seen in the last
/// five minutes (or the body's hash, with content deduplication) makes
/// it a duplicate: accepted, but not added again.
pub fn send(store: &Store, name: &str, message: NewMessage, now: u64) -> Result<Sent, Problem> {
    let queue = get(store, name)?;
    let dedup = message.dedup.clone().or_else(|| {
        (queue.settings.fifo && queue.settings.content_dedup)
            .then(|| md5_hex(message.body.as_bytes()))
    });
    if queue.settings.fifo
        && let Some(id) = &dedup
    {
        store.db().execute(
            "DELETE FROM queue_dedup WHERE queue = ?1 AND until <= ?2",
            params![name, now],
        )?;
        let seen: Option<(String, u64)> = store
            .db()
            .query_row(
                "SELECT message, seq FROM queue_dedup WHERE queue = ?1 AND id = ?2",
                params![name, id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        if let Some((id, sequence)) = seen {
            return Ok(Sent {
                id,
                sequence,
                duplicate: true,
            });
        }
    }
    let id = crate::protocol::request_id();
    let sequence = next_seq(store, name)?;
    // FIFO queues delay the whole queue, not each message.
    let delay = if queue.settings.fifo {
        queue.settings.delay
    } else {
        message.delay.unwrap_or(queue.settings.delay)
    };
    store.db().execute(
        "INSERT INTO queue_messages (id, queue, seq, body, attributes, grp, dedup, sent, visible_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            id,
            name,
            sequence,
            message.body,
            json(&message.attributes),
            message.group,
            dedup,
            now,
            now + delay * 1000
        ],
    )?;
    if queue.settings.fifo
        && let Some(dedup) = &dedup
    {
        store.db().execute(
            "INSERT OR REPLACE INTO queue_dedup (queue, id, until, message, seq) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![name, dedup, now + DEDUP_WINDOW_MS, id, sequence],
        )?;
    }
    Ok(Sent {
        id,
        sequence,
        duplicate: false,
    })
}

const MESSAGE: &str =
    "SELECT id, body, attributes, grp, dedup, seq, sent, receives, first_received, visible_at
     FROM queue_messages";

fn message_row(row: &Row) -> rusqlite::Result<StoredMessage> {
    let attributes: String = row.get(2)?;
    Ok(StoredMessage {
        id: row.get(0)?,
        body: row.get(1)?,
        attributes: serde_json::from_str(&attributes).unwrap_or(Value::Null),
        group: row.get(3)?,
        dedup: row.get(4)?,
        sequence: row.get(5)?,
        sent: row.get(6)?,
        receives: row.get(7)?,
        first_received: row.get(8)?,
        visible_at: row.get(9)?,
    })
}

/// Removes messages older than the queue keeps them, and moves those
/// received as often as its redrive allows to the dead-letter queue.
fn tidy(store: &Store, queue: &Queue, now: u64) -> Result<(), Problem> {
    let oldest = now.saturating_sub(queue.settings.retention * 1000);
    store.db().execute(
        "DELETE FROM queue_messages WHERE queue = ?1 AND sent < ?2",
        params![queue.name, oldest],
    )?;
    if let Some(redrive) = &queue.settings.redrive
        && find(store, &redrive.target)?.is_some()
    {
        let seq = next_seq(store, &redrive.target)?;
        store.db().execute(
            "UPDATE queue_messages SET queue = ?2, seq = ?4 + seq, receipt = NULL, visible_at = ?5
             WHERE queue = ?1 AND receives >= ?3 AND visible_at <= ?5",
            params![queue.name, redrive.target, redrive.max_receives, seq, now],
        )?;
    }
    Ok(())
}

/// Hands out up to `max` visible messages, oldest first, hidden for
/// `visibility` seconds. In a FIFO queue a group with a message in flight
/// gives out nothing until that one is deleted or comes back.
pub fn receive(
    store: &Store,
    name: &str,
    max: usize,
    visibility: u64,
    now: u64,
) -> Result<Vec<Received>, Problem> {
    let queue = get(store, name)?;
    tidy(store, &queue, now)?;
    let candidates = {
        let db = store.db();
        let mut query = db.prepare(&format!(
            "{MESSAGE} WHERE queue = ?1 AND visible_at <= ?2 ORDER BY seq"
        ))?;
        query
            .query_map(params![name, now], message_row)?
            .collect::<Result<Vec<_>, _>>()?
    };
    let busy: Vec<String> = if queue.settings.fifo {
        let db = store.db();
        let mut query = db.prepare(
            "SELECT DISTINCT grp FROM queue_messages
             WHERE queue = ?1 AND receives > 0 AND visible_at > ?2 AND grp IS NOT NULL",
        )?;
        query
            .query_map(params![name, now], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?
    } else {
        Vec::new()
    };
    let mut out = Vec::new();
    for mut message in candidates {
        if out.len() >= max {
            break;
        }
        if queue.settings.fifo
            && let Some(group) = &message.group
            && busy.contains(group)
        {
            continue;
        }
        let receipt = random("", 64);
        message.receives += 1;
        message.first_received.get_or_insert(now);
        message.visible_at = now + visibility * 1000;
        store.db().execute(
            "UPDATE queue_messages SET receives = ?2, first_received = ?3, visible_at = ?4, receipt = ?5
             WHERE id = ?1",
            params![
                message.id,
                message.receives,
                message.first_received,
                message.visible_at,
                receipt
            ],
        )?;
        out.push(Received { message, receipt });
    }
    Ok(out)
}

/// Deletes the message a receipt was given for. A receipt for a message
/// already gone is fine, as AWS has it.
pub fn delete_message(store: &Store, name: &str, receipt: &str) -> Result<(), Problem> {
    get(store, name)?;
    if receipt.len() < 16 {
        return Err(Problem::BadReceipt(receipt.to_owned()));
    }
    store.db().execute(
        "DELETE FROM queue_messages WHERE queue = ?1 AND receipt = ?2",
        params![name, receipt],
    )?;
    Ok(())
}

/// Hides a received message for `seconds` from now (0 shows it again).
pub fn change_visibility(
    store: &Store,
    name: &str,
    receipt: &str,
    seconds: u64,
    now: u64,
) -> Result<(), Problem> {
    get(store, name)?;
    let changed = store.db().execute(
        "UPDATE queue_messages SET visible_at = ?3
         WHERE queue = ?1 AND receipt = ?2 AND receives > 0 AND visible_at > ?4",
        params![name, receipt, now + seconds * 1000, now],
    )?;
    if changed == 0 {
        let known: Option<String> = store
            .db()
            .query_row(
                "SELECT id FROM queue_messages WHERE queue = ?1 AND receipt = ?2",
                params![name, receipt],
                |row| row.get(0),
            )
            .optional()?;
        return Err(match known {
            Some(_) => Problem::NotInFlight,
            None => Problem::BadReceipt(receipt.to_owned()),
        });
    }
    Ok(())
}

pub fn counts(store: &Store, name: &str, now: u64) -> Result<Counts, Problem> {
    Ok(store.db().query_row(
        "SELECT
            COALESCE(SUM(visible_at <= ?2), 0),
            COALESCE(SUM(visible_at > ?2 AND receives > 0), 0),
            COALESCE(SUM(visible_at > ?2 AND receives = 0), 0)
         FROM queue_messages WHERE queue = ?1",
        params![name, now],
        |row| {
            Ok(Counts {
                visible: row.get(0)?,
                in_flight: row.get(1)?,
                delayed: row.get(2)?,
            })
        },
    )?)
}

/// The first `limit` messages, in order, without receiving them.
pub fn peek(store: &Store, name: &str, limit: usize) -> Result<Vec<StoredMessage>, Problem> {
    get(store, name)?;
    let db = store.db();
    let mut query = db.prepare(&format!("{MESSAGE} WHERE queue = ?1 ORDER BY seq LIMIT ?2"))?;
    Ok(query
        .query_map(params![name, limit as i64], message_row)?
        .collect::<Result<Vec<_>, _>>()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> Store {
        let store = Store::open(None).unwrap();
        init(&store).unwrap();
        store
    }

    fn text(body: &str) -> NewMessage {
        NewMessage {
            body: body.into(),
            ..Default::default()
        }
    }

    fn queue(store: &Store, name: &str, settings: Settings) {
        create(store, name, settings, BTreeMap::new(), BTreeMap::new(), 0).unwrap();
    }

    #[test]
    fn a_message_comes_back_unless_deleted() {
        let store = store();
        queue(&store, "jobs", Settings::default());
        send(&store, "jobs", text("one"), 0).unwrap();

        let got = receive(&store, "jobs", 10, 30, 1_000).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].message.body, "one");
        // Hidden while in flight.
        assert!(receive(&store, "jobs", 10, 30, 2_000).unwrap().is_empty());
        assert_eq!(counts(&store, "jobs", 2_000).unwrap().in_flight, 1);
        // Back after the visibility timeout, with a new receipt.
        let again = receive(&store, "jobs", 10, 30, 32_000).unwrap();
        assert_eq!(again[0].message.receives, 2);
        assert_ne!(again[0].receipt, got[0].receipt);
        // Only the latest receipt deletes it.
        delete_message(&store, "jobs", &got[0].receipt).unwrap();
        assert_eq!(counts(&store, "jobs", 33_000).unwrap().in_flight, 1);
        delete_message(&store, "jobs", &again[0].receipt).unwrap();
        assert_eq!(counts(&store, "jobs", 33_000).unwrap(), Counts::default());
    }

    #[test]
    fn delays_and_visibility_changes() {
        let store = store();
        queue(&store, "later", Settings::default());
        let delayed = NewMessage {
            delay: Some(5),
            ..text("soon")
        };
        send(&store, "later", delayed, 0).unwrap();
        assert_eq!(counts(&store, "later", 1_000).unwrap().delayed, 1);
        assert!(receive(&store, "later", 1, 30, 1_000).unwrap().is_empty());
        let got = receive(&store, "later", 1, 30, 5_000).unwrap();
        change_visibility(&store, "later", &got[0].receipt, 0, 6_000).unwrap();
        assert_eq!(receive(&store, "later", 1, 30, 6_000).unwrap().len(), 1);
        assert_eq!(
            change_visibility(&store, "later", "no-such-receipt-at-all", 0, 7_000),
            Err(Problem::BadReceipt("no-such-receipt-at-all".into()))
        );
    }

    #[test]
    fn dead_letters_after_too_many_receives() {
        let store = store();
        queue(&store, "dlq", Settings::default());
        let settings = Settings {
            redrive: Some(Redrive {
                target: "dlq".into(),
                max_receives: 2,
            }),
            ..Settings::default()
        };
        queue(&store, "work", settings);
        assert_eq!(sources(&store, "dlq").unwrap(), ["work"]);
        send(&store, "work", text("poison"), 0).unwrap();
        receive(&store, "work", 1, 1, 0).unwrap();
        receive(&store, "work", 1, 1, 2_000).unwrap();
        // Received twice already: it moves on the next try.
        assert!(receive(&store, "work", 1, 1, 4_000).unwrap().is_empty());
        let dead = receive(&store, "dlq", 1, 30, 4_000).unwrap();
        assert_eq!(dead[0].message.body, "poison");
    }

    #[test]
    fn fifo_keeps_order_per_group_and_deduplicates() {
        let store = store();
        let settings = Settings {
            fifo: true,
            content_dedup: true,
            ..Settings::default()
        };
        queue(&store, "orders.fifo", settings);
        let in_group = |body: &str, group: &str| NewMessage {
            group: Some(group.into()),
            ..text(body)
        };
        let first = send(&store, "orders.fifo", in_group("a1", "a"), 0).unwrap();
        send(&store, "orders.fifo", in_group("a2", "a"), 0).unwrap();
        send(&store, "orders.fifo", in_group("b1", "b"), 0).unwrap();
        // Same body within five minutes: a duplicate, not a new message.
        let again = send(&store, "orders.fifo", in_group("a1", "a"), 1_000).unwrap();
        assert!(again.duplicate);
        assert_eq!(again.id, first.id);

        let got = receive(&store, "orders.fifo", 1, 30, 2_000).unwrap();
        assert_eq!(got[0].message.body, "a1");
        // Group a is busy, so b goes next, and a2 waits for a1.
        let next = receive(&store, "orders.fifo", 10, 30, 2_000).unwrap();
        let bodies: Vec<_> = next.iter().map(|r| r.message.body.as_str()).collect();
        assert_eq!(bodies, ["b1"]);
        delete_message(&store, "orders.fifo", &got[0].receipt).unwrap();
        let last = receive(&store, "orders.fifo", 10, 30, 3_000).unwrap();
        assert_eq!(last[0].message.body, "a2");
    }

    #[test]
    fn create_is_idempotent_with_the_same_settings() {
        let store = store();
        queue(&store, "q", Settings::default());
        queue(&store, "q", Settings::default());
        let other = Settings {
            delay: 9,
            ..Settings::default()
        };
        assert_eq!(
            create(&store, "q", other, BTreeMap::new(), BTreeMap::new(), 0),
            Err(Problem::QueueExists("q".into()))
        );
        assert_eq!(list(&store, "").unwrap().len(), 1);
        purge(&store, "q").unwrap();
        delete(&store, "q").unwrap();
        assert_eq!(get(&store, "q"), Err(Problem::NoQueue("q".into())));
    }
}
