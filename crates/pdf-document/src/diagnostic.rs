//! Read diagnostic types for the PDF reader.
//!
//! The types live in `pdf-object-reader` so lower layers can report recoverable
//! issues; this module keeps them reachable under the document reader.

pub use pdf_object_reader::diagnostic::{PdfReadDiagnostic, PdfReadDiagnosticKind};
pub use pdf_object_reader::object_id::ObjectId;
