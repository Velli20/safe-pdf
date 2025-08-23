use pdf_content_stream::error::PdfOperatorError;
use pdf_font::font::FontError;
use pdf_object::error::ObjectError;

use crate::{content_stream::ContentStreamReadError, resources::ResourcesError};
use thiserror::Error;

/// Defines errors that can occur when interpreting a PDF page object.
#[derive(Debug, Error)]
pub enum PageError {
    /// The page dictionary is missing `/Contents` entry.
    #[error("Missing /Contents entry in page dictionary")]
    MissingContent,

    /// The `/MediaBox` entry in the page dictionary is invalid.
    #[error("Invalid /MediaBox entry: {0}")]
    InvalidMediaBox(&'static str),

    /// Error originating from PDF object handling.
    #[error(transparent)]
    ObjectError(#[from] ObjectError),

    /// Error originating from content stream operators processing.
    #[error("Content stream operator error: {0}")]
    PdfOperatorError(#[from] PdfOperatorError),

    /// Error originating from font resources.
    #[error("Font resource error: {0}")]
    FontResourceError(#[from] FontError),

    /// Error originating from the page /Resources dictionary.
    #[error("Page /Resources error: {0}")]
    PageResourcesError(#[from] ResourcesError),

    /// Error while reading or interpreting the page content stream.
    #[error("Content stream read error: {0}")]
    ContentStreamReadError(#[from] ContentStreamReadError),

    /// Expected a dictionary for a named entry but found a different type.
    #[error("Expected a Dictionary object for {0}")]
    NotDictionary(&'static str),
}
