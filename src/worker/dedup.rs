//! Skips sending results that haven't changed since last time.
//!
//! Lists are refreshed every few seconds, but usually nothing changed.
//! Each message wakes the UI for a redraw, so dropping repeats keeps
//! IronStack idle while Docker is idle.

use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Mutex;

/// What a refreshed value is: one of the lists, or the engine version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    List(crate::docker::Resource),
    Version,
}

#[derive(Default)]
pub struct Dedup {
    last: Mutex<HashMap<Key, u64>>,
}

impl Dedup {
    /// Returns true if `value` differs from what was last sent under `key`,
    /// and remembers it.
    pub fn changed(&self, key: Key, value: &impl Hash) -> bool {
        let mut hasher = DefaultHasher::new();
        value.hash(&mut hasher);
        let hash = hasher.finish();
        let mut last = self.last.lock().unwrap_or_else(|e| e.into_inner());
        last.insert(key, hash) != Some(hash)
    }

    /// Forgets everything, so the next results are always sent. Used when
    /// the connection drops, because the UI clears its data then.
    pub fn reset(&self) {
        self.last.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::docker::Resource;

    #[test]
    fn only_reports_changes() {
        let dedup = Dedup::default();
        let key = Key::List(Resource::Volumes);
        assert!(dedup.changed(key, &vec!["a"]));
        assert!(!dedup.changed(key, &vec!["a"]));
        assert!(dedup.changed(key, &vec!["a", "b"]));
        assert!(dedup.changed(Key::Version, &"29.0"));
        dedup.reset();
        assert!(dedup.changed(key, &vec!["a", "b"]));
    }
}
