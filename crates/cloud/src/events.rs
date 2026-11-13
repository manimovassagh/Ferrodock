//! The record of requests: the last few thousand are kept, and each new
//! one is sent to whoever is listening (the app's request log).

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tokio::sync::broadcast;

/// How many requests are remembered.
const HISTORY: usize = 2_000;

/// One request and how it went.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Event {
    /// Counts up from 1, in the order requests finished.
    pub id: u64,
    /// When it arrived, in milliseconds since 1970.
    pub time: u64,
    /// The service's short name, or empty when none matched.
    pub service: String,
    /// The operation, when it's known.
    pub operation: String,
    pub method: String,
    pub path: String,
    pub status: u16,
    /// The AWS error code, for a request that failed.
    pub error: String,
    pub duration_ms: u64,
    pub region: String,
    /// What made the request: the CLI, an SDK, Terraform.
    pub user_agent: String,
    pub request_id: String,
}

pub struct Events {
    next: AtomicU64,
    history: Mutex<VecDeque<Arc<Event>>>,
    live: broadcast::Sender<Arc<Event>>,
}

impl Default for Events {
    fn default() -> Self {
        Self {
            next: AtomicU64::new(1),
            history: Mutex::new(VecDeque::with_capacity(HISTORY)),
            live: broadcast::channel(256).0,
        }
    }
}

impl Events {
    /// Records a request, giving it its ID.
    pub fn record(&self, mut event: Event) {
        // The ID is taken under the lock, so history stays in ID order.
        let Ok(mut history) = self.history.lock() else {
            return;
        };
        event.id = self.next.fetch_add(1, Ordering::Relaxed);
        let event = Arc::new(event);
        if history.len() == HISTORY {
            history.pop_front();
        }
        history.push_back(event.clone());
        drop(history);
        // Nobody listening is fine.
        let _ = self.live.send(event);
    }

    /// The remembered requests after `after`, and a feed of new ones.
    /// Subscribed under the lock, so none falls between the two.
    pub fn follow(&self, after: u64) -> (Vec<Arc<Event>>, broadcast::Receiver<Arc<Event>>) {
        let history = self.history.lock().unwrap_or_else(|p| p.into_inner());
        let feed = self.live.subscribe();
        let past = history.iter().filter(|e| e.id > after).cloned().collect();
        (past, feed)
    }

    pub fn clear(&self) {
        if let Ok(mut history) = self.history.lock() {
            history.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(path: &str) -> Event {
        Event {
            id: 0,
            time: 1,
            service: "s3".into(),
            operation: "ListBuckets".into(),
            method: "GET".into(),
            path: path.into(),
            status: 200,
            error: String::new(),
            duration_ms: 3,
            region: "us-east-1".into(),
            user_agent: "aws-cli/2".into(),
            request_id: "r".into(),
        }
    }

    #[tokio::test]
    async fn remembers_and_streams() {
        let events = Events::default();
        events.record(event("/a"));
        events.record(event("/b"));
        let (past, mut feed) = events.follow(0);
        assert_eq!(
            past.iter()
                .map(|e| (e.id, e.path.as_str()))
                .collect::<Vec<_>>(),
            [(1, "/a"), (2, "/b")]
        );
        // Only what came after the given ID.
        assert_eq!(events.follow(1).0.len(), 1);

        events.record(event("/c"));
        let live = feed.recv().await.unwrap();
        assert_eq!((live.id, live.path.as_str()), (3, "/c"));
    }

    #[test]
    fn forgets_the_oldest() {
        let events = Events::default();
        for i in 0..HISTORY + 5 {
            events.record(event(&format!("/{i}")));
        }
        let (past, _) = events.follow(0);
        assert_eq!(past.len(), HISTORY);
        assert_eq!(past[0].id, 6);
    }
}
