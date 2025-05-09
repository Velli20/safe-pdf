use pdf_parser::error::ParserError;

/// Errors that can occur while reading a PDF document.
#[derive(Debug)]
pub enum PdfError {
    /// Error reading the PDF document.
    ReadError(String),
    /// The document is missing a trailer.
    MissingTrailer,
    /// The document is missing a root object.
    MissingRoot,
    /// The document is missing a pages object.
    MissingPages,
    /// The document is missing a page tree.
    MissingPageTree,
    /// The document is missing a catalog.
    MissingCatalog,
    /// A page with the specified object number was not found.
    PageNotFound(i32),
}

impl From<ParserError> for PdfError {
    fn from(value: ParserError) -> Self {
        Self::ReadError(value.to_string())
    }
}
