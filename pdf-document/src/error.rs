use pdf_page::error::PageError;
use pdf_parser::error::ParserError;

/// Errors that can occur while reading a PDF document.
#[derive(Debug)]
pub enum PdfError {
    /// An error occurred during the parsing phase of the PDF document structure.
    ReadError(String),
    /// The PDF document trailer dictionary could not be found or is malformed.
    MissingTrailer,
    /// The `/Root` entry in the trailer, which points to the document catalog, is missing or invalid.
    MissingRoot,
    /// The `/Pages` entry in the document catalog is missing or invalid.
    MissingPages,
    /// The `Pages` entry in the document catalog is missing or invalid.
    MissingPageTree,
    /// The document catalog (the object referenced by `/Root` in the trailer) is missing or invalid.
    MissingCatalog,
    /// A specific page object, referenced by its object number, could not be found or is not a valid page dictionary.
    PageNotFound(i32),
}

impl From<ParserError> for PdfError {
    fn from(value: ParserError) -> Self {
        Self::ReadError(value.to_string())
    }
}

impl From<PageError> for PdfError {
    fn from(value: PageError) -> Self {
        Self::ReadError(value.to_string())
    }
}
