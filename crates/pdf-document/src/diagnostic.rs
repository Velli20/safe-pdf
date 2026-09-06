//! Read diagnostic types for the PDF reader.
//!
//! This module defines the recoverable issues that can be reported while
//! reading a PDF, together with the contextual information attached to each
//! diagnostic.

use pdf_object_reader::object_id::ObjectId;

/// Categorizes a recoverable PDF read problem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdfReadDiagnosticKind {
    /// The optional encryption dictionary could not be used.
    MalformedEncryption,
    /// An optional indirect object could not be parsed.
    ObjectParse,
    /// An optional object's encrypted data could not be decrypted.
    ObjectDecryption,
    /// A compressed object or its containing object stream could not be read.
    CompressedObject,
}

/// Describes a recoverable problem encountered while reading a PDF.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfReadDiagnostic {
    /// The category of recoverable problem.
    pub kind: PdfReadDiagnosticKind,
    /// The byte offset associated with the problem, when available.
    pub byte_offset: Option<usize>,
    /// The indirect object associated with the problem, when available.
    pub object: Option<ObjectId>,
    /// A human-readable rendering of the underlying error.
    pub message: String,
}

impl PdfReadDiagnostic {
    /// Creates a diagnostic with the available PDF location context.
    pub(crate) fn new(
        kind: PdfReadDiagnosticKind,
        byte_offset: Option<usize>,
        object: Option<ObjectId>,
        error: impl std::fmt::Display,
    ) -> Self {
        Self {
            kind,
            byte_offset,
            object,
            message: error.to_string(),
        }
    }
}
