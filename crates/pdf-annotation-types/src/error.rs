use pdf_object::error::ObjectError;
use pdf_resources::error::PdfPagesError;
use thiserror::Error;

/// Errors that can occur while parsing page annotations.
#[derive(Debug, Error)]
pub enum AnnotationError {
    /// An error returned while reading or resolving a PDF object.
    #[error("{0}")]
    Object(#[from] ObjectError),
    /// An error returned while parsing appearance resources or content streams.
    #[error("{0}")]
    Resources(#[from] PdfPagesError),
    /// An annotation entry is present but has the wrong type or value shape.
    #[error("invalid annotation entry '/{entry}': {reason}")]
    InvalidEntry { entry: &'static str, reason: String },
    /// A required annotation entry is missing.
    #[error("missing required annotation entry '/{entry}'")]
    MissingEntry { entry: &'static str },
}
