//! Small thread-safe caches used by immutable font faces.

use std::{collections::HashMap, hash::Hash, sync::RwLock};

/// Stores lazily computed font query results behind a reader/writer lock.
///
/// Font faces implement `Send + Sync`, so query caches need interior
/// synchronization. Reads dominate after the first lookup, which makes an
/// [`RwLock`] preferable to serializing all access through a mutex.
pub(crate) struct QueryCache<K, V> {
    /// Values computed for keys already requested by layout or rendering.
    entries: RwLock<HashMap<K, V>>,
}

impl<K, V> QueryCache<K, V>
where
    K: Eq + Hash,
    V: Copy,
{
    /// Creates an empty cache without doing eager font-wide work.
    pub(crate) fn new() -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
        }
    }

    /// Returns the cached value for `key`, if that query has run before.
    pub(crate) fn get(&self, key: &K) -> Option<V> {
        // A panic in unrelated client code could poison the lock. The cache
        // contains only independent completed values, so the inner map remains
        // safe to read instead of turning poisoning into a rendering failure.
        match self.entries.read() {
            Ok(entries) => entries.get(key).copied(),
            Err(poisoned) => poisoned.into_inner().get(key).copied(),
        }
    }

    /// Records a completed query result for later calls.
    pub(crate) fn insert(&self, key: K, value: V) {
        // Recovering a poisoned cache is safe for the same reason as in `get`:
        // entries are inserted only after their computation has completed.
        match self.entries.write() {
            Ok(mut entries) => {
                entries.insert(key, value);
            }
            Err(poisoned) => {
                poisoned.into_inner().insert(key, value);
            }
        }
    }
}
