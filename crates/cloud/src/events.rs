//! The record of requests: the last thousand are kept, each with its
//! headers and the start of its bodies, and each new one is sent to
//! whoever is listening (the app's request log).

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tokio::sync::broadcast;

use crate::capture::Detail;

/// How many requests are remembered. With [`crate::capture::BODY_LIMIT`]
/// this bounds the memory the log can take, to about 32 MiB.
const HISTORY: usize = 1_000;

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
    /// What it was about (a role's name, a queue's URL), when its fields
    /// say.
    pub resource: String,
    pub method: String,
    pub path: String,
    pub status: u16,
    /// The AWS error code, for a request that failed.
    pub error: String,
    /// What the error said.
    pub message: String,
    pub duration_ms: u64,
    pub region: String,
    /// What made the request: the CLI, an SDK, Terraform.
    pub user_agent: String,
    pub request_id: String,
}

pub struct Events {
    next: AtomicU64,
    history: Mutex<VecDeque<(Arc<Event>, Arc<Detail>)>>,
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
    pub fn record(&self, mut event: Event, detail: Detail) {
        // The ID is taken under the lock, so history stays in ID order.
        let Ok(mut history) = self.history.lock() else {
            return;
        };
        event.id = self.next.fetch_add(1, Ordering::Relaxed);
        let event = Arc::new(event);
        if history.len() == HISTORY {
            history.pop_front();
        }
        history.push_back((event.clone(), Arc::new(detail)));
        drop(history);
        // Nobody listening is fine.
        let _ = self.live.send(event);
    }

    /// The remembered requests after `after`, and a feed of new ones.
    /// Subscribed under the lock, so none falls between the two.
    pub fn follow(&self, after: u64) -> (Vec<Arc<Event>>, broadcast::Receiver<Arc<Event>>) {
        let history = self.history.lock().unwrap_or_else(|p| p.into_inner());
        let feed = self.live.subscribe();
        let past = Self::newer(&history, after);
        (past, feed)
    }

    fn newer(history: &VecDeque<(Arc<Event>, Arc<Detail>)>, after: u64) -> Vec<Arc<Event>> {
        history
            .iter()
            .filter(|(event, _)| event.id > after)
            .map(|(event, _)| event.clone())
            .collect()
    }

    /// The remembered requests after `after`, oldest first.
    pub fn after(&self, after: u64) -> Vec<Arc<Event>> {
        Self::newer(
            &self.history.lock().unwrap_or_else(|p| p.into_inner()),
            after,
        )
    }

    /// The ID of the newest request so far; 0 before the first.
    pub fn last(&self) -> u64 {
        self.next.load(Ordering::Relaxed) - 1
    }

    /// A remembered request with its headers and bodies.
    pub fn detail(&self, id: u64) -> Option<(Arc<Event>, Arc<Detail>)> {
        let history = self.history.lock().unwrap_or_else(|p| p.into_inner());
        history.iter().find(|(event, _)| event.id == id).cloned()
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
            resource: String::new(),
            method: "GET".into(),
            path: path.into(),
            status: 200,
            error: String::new(),
            message: String::new(),
            duration_ms: 3,
            region: "us-east-1".into(),
            user_agent: "aws-cli/2".into(),
            request_id: "r".into(),
        }
    }

    #[tokio::test]
    async fn remembers_and_streams() {
        let events = Events::default();
        events.record(event("/a"), Detail::default());
        events.record(event("/b"), Detail::default());
        let (past, mut feed) = events.follow(0);
        assert_eq!(
            past.iter()
                .map(|e| (e.id, e.path.as_str()))
                .collect::<Vec<_>>(),
            [(1, "/a"), (2, "/b")]
        );
        // Only what came after the given ID.
        assert_eq!(events.follow(1).0.len(), 1);
        assert_eq!(events.after(1)[0].path, "/b");
        assert_eq!(events.detail(2).unwrap().0.path, "/b");
        assert!(events.detail(9).is_none());

        events.record(event("/c"), Detail::default());
        let live = feed.recv().await.unwrap();
        assert_eq!((live.id, live.path.as_str()), (3, "/c"));
    }

    #[test]
    fn forgets_the_oldest() {
        let events = Events::default();
        for i in 0..HISTORY + 5 {
            events.record(event(&format!("/{i}")), Detail::default());
        }
        let (past, _) = events.follow(0);
        assert_eq!(past.len(), HISTORY);
        assert_eq!(past[0].id, 6);
    }
}
