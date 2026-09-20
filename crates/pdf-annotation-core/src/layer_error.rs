//! Errors at the backend-neutral annotation layer boundary.

/// Failure while importing, projecting, or editing annotations for a host.
#[derive(Debug, thiserror::Error)]
pub enum AnnotationLayerError {
    /// Core rejected a semantic edit or imported record; the source retains its context.
    #[error(transparent)]
    Core(#[from] crate::error::CoreError<crate::session_store::SessionStorageError>),
    /// Core geometry or metadata validation failed before dispatch.
    #[error(transparent)]
    Validation(#[from] crate::error::ValidationError),
    /// Host coordinate conversion failed.
    #[error(transparent)]
    Transform(#[from] pdf_graphics::transform::TransformError),
    /// Invalid host geometry or event input.
    #[error("invalid annotation input: {0}")]
    InvalidInput(&'static str),
    /// The requested page index lies beyond the imported document.
    #[error("annotation page {0} is out of range")]
    PageOutOfRange(u32),
    /// A mounted control's command names a different annotation, field, or capability.
    #[error("command does not match the mounted annotation")]
    TargetMismatch,
    /// Core committed a change of a kind this layer did not dispatch.
    #[error("unexpected Core change for the dispatched command")]
    UnexpectedChange,
    /// Host identifiers or resources exceed their supported range.
    #[error("annotation resource limit exceeded")]
    ResourceLimit,
    /// An event or prepared page belongs to an obsolete revision.
    #[error("stale annotation or viewport revision")]
    StaleRevision,
}

/// Result for annotation layer APIs.
pub type AnnotationLayerResult<T> = Result<T, AnnotationLayerError>;
