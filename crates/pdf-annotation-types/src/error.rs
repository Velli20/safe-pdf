use pdf_object::error::ObjectError;
use pdf_resources::error::PdfPagesError;
use thiserror::Error;

use crate::annotation_id::AnnotationId;

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
    #[error("invalid annotation entry '/{entry:?}': {reason}")]
    InvalidEntry {
        entry: &'static [u8],
        reason: String,
    },
    /// A required annotation entry is missing.
    #[error("missing required annotation entry '/{entry:?}'")]
    MissingEntry { entry: &'static [u8] },
}

/// Errors returned while resolving a button widget's active appearance state.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ButtonStateError {
    /// The annotation has no non-`/Off` entry in its normal appearance dictionary.
    #[error("annotation {} has no usable non-/Off normal appearance state", id.get())]
    MissingOnState {
        /// Stable, page-scoped identifier of the annotation whose state could not be resolved.
        id: AnnotationId,
    },
}
