use pdf_object::error::ObjectError;
use pdf_page::pages::PdfPagesError;
use pdf_parser::{error::ParserError, header::HeaderError};
use thiserror::Error;

/// Errors that can occur while reading a PDF document.
#[derive(Debug, Error)]
pub enum PdfError {
    /// An error occurred during the parsing phase of the PDF document structure.
    #[error("read error: {0}")]
    ReadError(String),
    /// The PDF document trailer dictionary could not be found or is malformed.
    #[error("missing trailer")]
    MissingTrailer,
    /// The `/Root` entry in the trailer, which points to the document catalog, is missing or invalid.
    #[error("missing root")]
    MissingRoot,
    /// The `/Pages` entry in the document catalog is missing or invalid.
    #[error("missing pages")]
    MissingPages,
    /// The `Pages` entry in the document catalog is missing or invalid.
    #[error("missing page tree")]
    MissingPageTree,
    /// The document catalog (the object referenced by `/Root` in the trailer) is missing or invalid.
    #[error("missing catalog")]
    MissingCatalog,
    #[error("missing type")]
    MissingType,
    #[error("{0}")]
    ObjectError(#[from] ObjectError),
    #[error("{0}")]
    PdfPagesError(#[from] PdfPagesError),
    #[error("{0}")]
    ParserError(#[from] ParserError),
    #[error("Error parsing PDF header: {0}")]
    HeaderError(#[from] HeaderError),
}
