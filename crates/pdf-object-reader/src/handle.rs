//! Nonblocking typed resource handles with explicit readiness and ownership.

use std::sync::{Arc, Weak};

use crate::cache::TypedEntry;
use crate::error::{ObjectReadError, ReadResult};
use crate::object_id::ObjectId;

/// A shareable reference to a typed object, possibly still under construction.
///
/// Indirect handles weakly reference entries owned by the reader. Store these
/// handles for graph edges so recursive resources do not create strong ownership
/// cycles. Destroying the reader expires those handles, even if some separately
/// acquired values remain alive through their own Arc pointers.
///
/// Direct inputs instead produce owned, ready handles and bypass the typed
/// cache. Cloning a handle never requires T to implement Clone. A handle is
/// Send + Sync when T is Send + Sync.
///
/// Access never waits for a decoder to finish. Do not dereference a recursive
/// pending edge while constructing its value. This API intentionally has no
/// Deref implementation or automatic blocking resolution.
pub struct ObjectHandle<T> {
    storage: HandleStorage<T>,
}

enum HandleStorage<T> {
    Direct(Arc<T>),
    Indirect {
        object_id: ObjectId,
        entry: Weak<TypedEntry<T>>,
    },
}

impl<T> Clone for ObjectHandle<T> {
    fn clone(&self) -> Self {
        Self {
            storage: match &self.storage {
                HandleStorage::Direct(value) => HandleStorage::Direct(Arc::clone(value)),
                HandleStorage::Indirect { object_id, entry } => HandleStorage::Indirect {
                    object_id: *object_id,
                    entry: Weak::clone(entry),
                },
            },
        }
    }
}

impl<T> ObjectHandle<T> {
    /// Returns the requested indirect identity, or None for a direct value.
    ///
    /// Identity remains available after the reader has been destroyed.
    pub fn object_id(&self) -> Option<ObjectId> {
        match &self.storage {
            HandleStorage::Direct(_) => None,
            HandleStorage::Indirect { object_id, .. } => Some(*object_id),
        }
    }

    /// Returns None while loading, a completed value, or the recorded failure.
    ///
    /// An expired reader produces HandleExpired, not None. Returned values can
    /// outlive the reader, but their indirect graph edges still need the reader.
    pub fn try_get(&self) -> ReadResult<Option<Arc<T>>> {
        match &self.storage {
            HandleStorage::Direct(value) => Ok(Some(Arc::clone(value))),
            HandleStorage::Indirect { object_id, entry } => entry
                .upgrade()
                .ok_or(ObjectReadError::HandleExpired {
                    object_id: *object_id,
                })?
                .value(),
        }
    }

    /// Requires a completed value, returning ObjectPending while still loading.
    ///
    /// Failed loads preserve their original error source. This method never
    /// triggers decoding, waits for publication, or retries a failed load.
    pub fn get(&self) -> ReadResult<Arc<T>> {
        match &self.storage {
            HandleStorage::Direct(value) => Ok(Arc::clone(value)),
            HandleStorage::Indirect { object_id, .. } => {
                self.try_get()?.ok_or(ObjectReadError::ObjectPending {
                    object_id: *object_id,
                })
            }
        }
    }

    pub(crate) fn direct(value: Arc<T>) -> Self {
        Self {
            storage: HandleStorage::Direct(value),
        }
    }

    pub(crate) fn indirect(id: ObjectId, entry: Weak<TypedEntry<T>>) -> Self {
        Self {
            storage: HandleStorage::Indirect {
                object_id: id,
                entry,
            },
        }
    }
}

impl<T> From<T> for ObjectHandle<T> {
    fn from(value: T) -> Self {
        Self::direct(Arc::new(value))
    }
}
