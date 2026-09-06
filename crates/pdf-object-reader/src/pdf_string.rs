//! PDF byte strings.

use crate::string_kind::StringKind;
use std::sync::Arc;

/// Stores a PDF byte string and its source representation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfString {
    bytes: Arc<[u8]>,
    kind: StringKind,
}

impl PdfString {
    /// Creates a shared PDF string from bytes and source syntax metadata.
    pub fn new(bytes: impl AsRef<[u8]>, kind: StringKind) -> Self {
        Self {
            bytes: Arc::from(bytes.as_ref()),
            kind,
        }
    }

    /// Returns the uninterpreted bytes stored in the string.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Returns the syntax used to represent the string in the source PDF.
    pub fn kind(&self) -> StringKind {
        self.kind
    }
}
