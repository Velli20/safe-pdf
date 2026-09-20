//! Session-only storage boundary; no operation claims durable persistence.
use crate::{models::DocumentId, store::AnnotationStore};

/// Persistence is unavailable in this session-only host.
#[derive(Debug, thiserror::Error)]
#[error("annotation persistence is unavailable in this browser session")]
pub struct SessionStorageError;

/// Stateless store used by each open document's in-memory engine.
#[derive(Default)]
pub struct SessionStore;
impl AnnotationStore for SessionStore {
    type Error = SessionStorageError;

    /// Returns absence for every document; never reads external storage or fails.
    fn load(&mut self, _document: &DocumentId) -> Result<Option<Vec<u8>>, Self::Error> {
        Ok(None)
    }

    /// Rejects saving any bytes with `SessionStorageError`; never acknowledges a write.
    fn save(&mut self, _document: &DocumentId, _bytes: &[u8]) -> Result<(), Self::Error> {
        Err(SessionStorageError)
    }
}
