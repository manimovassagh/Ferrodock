//! Message queues: messages that wait until a consumer takes them, stay
//! hidden while it works on them, and come back if it doesn't finish.
//! Nothing here knows about AWS: SQS is one way in, and other clouds'
//! queues can be others.
//!
//! - `records.rs`: queues and messages in SQLite, and the rules: delay,
//!   visibility, retention, first-in-first-out groups, deduplication,
//!   and moving messages received too often to a dead-letter queue.

pub mod records;

pub use records::{Counts, NewMessage, Problem, Queue, Received, Settings, StoredMessage};
