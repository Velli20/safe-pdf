//! Read diagnostic types shared by every PDF reading layer.
//!
//! This module defines the recoverable issues that can be reported while
//! reading a PDF, together with the contextual information attached to each
//! diagnostic.

use crate::object_id::ObjectId;

/// Categorizes a recoverable PDF read problem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum PdfReadDiagnosticKind {
    /// The optional encryption dictionary could not be used.
    MalformedEncryption,
    /// An optional indirect object could not be parsed.
    ObjectParse,
    /// An optional object's encrypted data could not be decrypted.
    ObjectDecryption,
    /// A compressed object or its containing object stream could not be read.
    CompressedObject,
    /// A page annotation, or the field group it belongs to, could not be adopted by the
    /// annotation layer.
    AnnotationImport,
}

/// Describes a recoverable problem encountered while reading a PDF.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PdfReadDiagnostic {
    /// The category of recoverable problem.
    pub kind: PdfReadDiagnosticKind,
    /// The zero-based page index associated with the problem, when available.
    pub page: Option<u32>,
    /// The byte offset associated with the problem, when available.
    pub byte_offset: Option<usize>,
    /// The indirect object associated with the problem, when available.
    pub object: Option<ObjectId>,
    /// A human-readable rendering of the underlying error.
    pub message: String,
}

impl PdfReadDiagnostic {
    /// Creates a diagnostic with the available PDF location context.
    pub fn new(
        kind: PdfReadDiagnosticKind,
        byte_offset: Option<usize>,
        object: Option<ObjectId>,
        error: impl std::fmt::Display,
    ) -> Self {
        Self {
            kind,
            page: None,
            byte_offset,
            object,
            message: error.to_string(),
        }
    }

    /// Attaches the zero-based page index the problem was encountered on.
    pub fn on_page(mut self, page: u32) -> Self {
        self.page = Some(page);
        self
    }
}
