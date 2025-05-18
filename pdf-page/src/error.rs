use pdf_font::error::FontError;
use pdf_object::error::ObjectError;
use pdf_operator::error::PdfOperatorError;

/// Defines errors that can occur when interpreting a PDF page object.
#[derive(Debug, Clone, PartialEq)]
pub enum PageError {
    /// The page dictionary is missing `/Contents` entry.
    MissingContent,
    /// The page is missing required `/MediaBox` entry.
    MissingMediaBox,
    /// The page is missing required `/Resorces` entry.
    MissingResources,
    /// The `/MediaBox` entry in the page dictionary is invalid.
    InvalidMediaBox(&'static str),
    /// Wraps an error message from an `ObjectError`
    /// encountered while processing a PDF object related to the page.
    ObjectError(String),
    /// Wraps an error message from a `PdfPainterError`
    PdfOperatorError(String),
    /// Wraps an error message from a `FontError`.
    FontResourceError(String),
    NotDictionary(&'static str),
}

impl From<ObjectError> for PageError {
    fn from(err: ObjectError) -> Self {
        Self::ObjectError(err.to_string())
    }
}

impl From<PdfOperatorError> for PageError {
    fn from(err: PdfOperatorError) -> Self {
        Self::PdfOperatorError(err.to_string())
    }
}

impl From<FontError> for PageError {
    fn from(err: FontError) -> Self {
        Self::FontResourceError(err.to_string())
    }
}

impl std::fmt::Display for PageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PageError::MissingContent => {
                write!(f, "Missing `/Contents` entry")
            }
            PageError::MissingMediaBox => {
                write!(f, "Missing `/MediaBox` entry")
            }
            PageError::MissingResources => {
                write!(f, "Missing `/MissingResources` entry")
            }
            PageError::InvalidMediaBox(err) => {
                write!(f, "Invalid `/MediaBox` entry: {}", err)
            }
            PageError::ObjectError(err) => {
                write!(
                    f,
                    "Failed to process a PDF object related to the page: {}",
                    err
                )
            }
            PageError::PdfOperatorError(err) => {
                write!(
                    f,
                    "Failed to process a PDF object related to the page: {}",
                    err
                )
            }
            PageError::FontResourceError(err) => {
                write!(f, "Error loading font resource: {}", err)
            }
            PageError::NotDictionary(name) => {
                write!(f, "Expected a Dictionary object for {}", name)
            }
        }
    }
}
