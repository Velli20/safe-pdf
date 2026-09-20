use crate::models::DocumentId;

/// Synchronous persistence supplied by the host, independent of rendering.
///
/// Core serializes complete sidecars as JSON bytes. Successful saves must
/// atomically replace the document's prior bytes. Failed saves must preserve
/// the prior bytes. Durability beyond acknowledgement is backend-defined.
/// Hosts must serialize access to a document across engine instances; this
/// interface does not provide cross-process compare-and-swap or conflict checks.
pub trait AnnotationStore {
    /// Backend failure retained as the source of a Core storage error.
    type Error: std::error::Error + Send + Sync + 'static;

    /// Load a complete sidecar, or return `None` if this document has none.
    /// `document` is the host's stable document identity.
    ///
    /// # Errors
    /// Return the backend error when reading fails; absence is not an error.
    /// Implementations should report failures without panicking.
    fn load(&mut self, document: &DocumentId) -> Result<Option<Vec<u8>>, Self::Error>;

    /// Atomically replace the sidecar with these bytes before acknowledging it.
    /// `document` identifies the sidecar; `bytes` contains the entire JSON snapshot.
    ///
    /// # Errors
    /// Return the backend error when replacement fails, preserving prior bytes.
    /// Implementations should report failures without panicking.
    fn save(&mut self, document: &DocumentId, bytes: &[u8]) -> Result<(), Self::Error>;
}
