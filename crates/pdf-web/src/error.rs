//! Errors at the browser orchestration and interaction boundary.

/// Failure while preparing a page, browser presentation, or host interaction.
#[derive(Debug, thiserror::Error)]
pub enum WebError {
    /// The backend-neutral annotation layer rejected an import, projection, or edit.
    #[error(transparent)]
    Annotations(#[from] pdf_annotation_core::AnnotationLayerError),
    /// Browser wire serialization or decoding failed.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    /// Invalid host geometry or event input.
    #[error("invalid web input: {0}")]
    InvalidInput(&'static str),
    /// Host identifiers or resources exceed their supported range.
    #[error("web resource limit exceeded")]
    ResourceLimit,
    /// A Canvas 2D operation failed.
    #[error(transparent)]
    Backend(#[from] pdf_graphics_web::WebCanvasBackendError),
    /// PDF interpretation failed.
    #[error(transparent)]
    Canvas(#[from] pdf_canvas::error::PdfCanvasError),
    /// Shared viewport validation failed.
    #[error(transparent)]
    Viewport(#[from] pdf_canvas::ViewportError),
    /// Host coordinate conversion failed.
    #[error(transparent)]
    Transform(#[from] pdf_graphics::transform::TransformError),
    /// A renderer-owned text-selection operation failed.
    #[error(transparent)]
    TextSelection(#[from] pdf_renderer::TextSelectionError),
}

/// Result for browser orchestration and interaction APIs.
pub type WebResult<T> = Result<T, WebError>;
