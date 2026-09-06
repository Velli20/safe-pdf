//! Completed raw-object caching and reader-owned typed resource entries.
//!
//! Raw cache implementations are public extension points. Typed coordination is
//! private: clients implement decoders, not placeholder or cache-management traits.

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::error::Error;
use std::sync::{Arc, RwLock};

use thiserror::Error;

use crate::error::{ObjectReadError, ReadResult};
use crate::handle::ObjectHandle;
use crate::object_id::ObjectId;
use crate::pdf_object::PdfObject;

/// Stores completed immutable raw objects for one document snapshot.
///
/// Implementations must support concurrent calls and atomic `insert_if_absent`.
/// They must not coordinate in-flight source loads: duplicate acquisition is
/// permitted, and no cache lock may be held while invoking the source.
/// Each instance must be isolated from documents with overlapping object IDs.
pub trait ObjectCache: Send + Sync {
    /// The implementation-specific synchronization or storage error.
    type Error: Error + Send + Sync + 'static;

    /// Returns a completed raw object when present.
    fn get(&self, object_id: ObjectId) -> Result<Option<PdfObject>, Self::Error>;

    /// Publishes a completed value and returns the canonical cached handle.
    ///
    /// If another insertion won the race, return that existing value.
    fn insert_if_absent(
        &self,
        object_id: ObjectId,
        object: PdfObject,
    ) -> Result<PdfObject, Self::Error>;
}

/// Reports synchronization failures in the default raw cache.
#[derive(Debug, Error)]
pub enum ConcurrentObjectCacheError {
    /// A previous panic poisoned the lock needed for an operation.
    #[error("object cache lock was poisoned during {operation}")]
    LockPoisoned {
        /// The operation that could not acquire the lock.
        operation: &'static str,
    },
}

/// A standard-library raw cache containing only completed immutable objects.
///
/// The reader owns this cache; each instance belongs to one document snapshot.
#[derive(Debug)]
pub struct ConcurrentObjectCache {
    entries: RwLock<HashMap<ObjectId, PdfObject>>,
}

impl ConcurrentObjectCache {
    /// Creates an empty raw cache.
    pub fn new() -> Self {
        Self::with_capacity(0)
    }

    /// Creates a raw cache with reserved capacity for completed objects.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            entries: RwLock::new(HashMap::with_capacity(capacity)),
        }
    }
}

impl Default for ConcurrentObjectCache {
    fn default() -> Self {
        Self::new()
    }
}

impl ObjectCache for ConcurrentObjectCache {
    type Error = ConcurrentObjectCacheError;

    fn get(&self, object_id: ObjectId) -> Result<Option<PdfObject>, Self::Error> {
        let entries = self
            .entries
            .read()
            .map_err(|_| ConcurrentObjectCacheError::LockPoisoned { operation: "get" })?;
        Ok(entries.get(&object_id).cloned())
    }

    fn insert_if_absent(
        &self,
        object_id: ObjectId,
        object: PdfObject,
    ) -> Result<PdfObject, Self::Error> {
        let mut entries =
            self.entries
                .write()
                .map_err(|_| ConcurrentObjectCacheError::LockPoisoned {
                    operation: "insert",
                })?;
        Ok(entries.entry(object_id).or_insert(object).clone())
    }
}

/// Identifies one interpretation of an indirect object within a reader.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct TypedKey {
    object_id: ObjectId,
    output_type: TypeId,
}

/// Owns all typed entries until the reader is destroyed.
///
/// Erased entries are actually Arc<TypedEntry<T>> and are recovered by checked
/// Any downcasts. There is no eviction or retry API for a reader.
pub(crate) struct TypedObjectCache {
    entries: RwLock<HashMap<TypedKey, Arc<dyn Any + Send + Sync>>>,
}

impl TypedObjectCache {
    pub(crate) fn new() -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
        }
    }

    /// Atomically reserves before decoding, or observes an existing entry.
    ///
    /// An occupied failed entry reports its stored failure. An occupied loading
    /// entry yields a weak handle immediately without waiting for its owner.
    pub(crate) fn reserve<T: Send + Sync + 'static>(
        &self,
        object_id: ObjectId,
    ) -> ReadResult<Reservation<T>> {
        let mut entries = self
            .entries
            .write()
            .map_err(|_| ObjectReadError::TypedCache {
                operation: "reserve",
                source: Box::new(ConcurrentObjectCacheError::LockPoisoned {
                    operation: "reserve",
                }),
            })?;
        let key = TypedKey {
            object_id,
            output_type: TypeId::of::<T>(),
        };
        if let Some(entry) = entries.get(&key) {
            let entry = Arc::clone(entry).downcast::<TypedEntry<T>>().map_err(|_| {
                ObjectReadError::TypedCache {
                    operation: "downcast",
                    source: Box::new(std::io::Error::other(
                        "typed cache entry has an inconsistent type",
                    )),
                }
            })?;
            let handle = ObjectHandle::indirect(object_id, Arc::downgrade(&entry));
            // Surface a stored failure, while leaving pending entries nonblocking.
            handle.try_get()?;
            return Ok(Reservation::Existing(handle));
        }
        let entry = Arc::new(TypedEntry {
            object_id,
            state: RwLock::new(EntryState::Loading),
        });
        entries.insert(key, Arc::<TypedEntry<T>>::clone(&entry));
        Ok(Reservation::Vacant(TypedReservation {
            entry,
            completed: false,
        }))
    }
}

pub(crate) enum Reservation<T> {
    Existing(ObjectHandle<T>),
    Vacant(TypedReservation<T>),
}

/// The reader retains this allocation; graph edges keep weak handles to it.
pub(crate) struct TypedEntry<T> {
    pub(crate) object_id: ObjectId,
    pub(crate) state: RwLock<EntryState<T>>,
}

/// Only the reservation owner can publish a terminal state.
pub(crate) enum EntryState<T> {
    Loading,
    Ready(Arc<T>),
    Failed(Arc<ObjectReadError>),
}

/// Owns the right to finish an entry without holding a cache or entry lock.
///
/// Drop must mark an unfinished reservation as aborted without panicking or
/// waiting on another decoder, including when a decoder unwinds.
pub(crate) struct TypedReservation<T> {
    entry: Arc<TypedEntry<T>>,
    completed: bool,
}

impl<T> TypedReservation<T> {
    /// Exposes a weak pending handle before invoking a recursive decoder.
    pub(crate) fn handle(&self) -> ObjectHandle<T> {
        ObjectHandle::indirect(self.entry.object_id, Arc::downgrade(&self.entry))
    }

    /// Publishes a completed value and disarms abort-on-drop behavior.
    pub(crate) fn publish(mut self, value: Arc<T>) -> ReadResult<ObjectHandle<T>> {
        *self
            .entry
            .state
            .write()
            .map_err(|_| ObjectReadError::TypedCache {
                operation: "publish",
                source: Box::new(ConcurrentObjectCacheError::LockPoisoned {
                    operation: "publish",
                }),
            })? = EntryState::Ready(value);
        self.completed = true;
        Ok(self.handle())
    }

    /// Retains a shared failure so every existing handle observes the same cause.
    pub(crate) fn fail(mut self, error: Arc<ObjectReadError>) -> ReadResult<()> {
        *self
            .entry
            .state
            .write()
            .map_err(|_| ObjectReadError::TypedCache {
                operation: "fail",
                source: Box::new(ConcurrentObjectCacheError::LockPoisoned { operation: "fail" }),
            })? = EntryState::Failed(error);
        self.completed = true;
        Ok(())
    }
}

impl<T> Drop for TypedReservation<T> {
    fn drop(&mut self) {
        if !self.completed {
            let mut state = match self.entry.state.write() {
                Ok(state) => state,
                Err(poisoned) => poisoned.into_inner(),
            };
            *state = EntryState::Failed(Arc::new(ObjectReadError::LoadAborted {
                object_id: self.entry.object_id,
            }));
        }
    }
}
