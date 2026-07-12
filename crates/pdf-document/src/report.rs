//! Read report types for the PDF reader.
//!
//! This module groups the successful read result and the recoverable diagnostics
//! collected while loading a document best-effort.

use crate::document::PdfDocument;

use super::diagnostic::PdfReadDiagnostic;

/// A successfully read document and the recoverable problems encountered while reading it.
pub struct PdfReadReport {
    document: PdfDocument,
    diagnostics: Vec<PdfReadDiagnostic>,
}

impl PdfReadReport {
    /// Creates a report from the loaded document and any recoverable diagnostics.
    pub(crate) fn new(document: PdfDocument, diagnostics: Vec<PdfReadDiagnostic>) -> Self {
        Self {
            document,
            diagnostics,
        }
    }

    /// Returns the usable document produced by the best-effort read.
    pub fn document(&self) -> &PdfDocument {
        &self.document
    }

    /// Returns recoverable problems encountered while reading the document.
    pub fn diagnostics(&self) -> &[PdfReadDiagnostic] {
        &self.diagnostics
    }

    /// Consumes the report and returns its document.
    pub fn into_document(self) -> PdfDocument {
        self.document
    }
}
