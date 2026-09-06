//! PDF byte strings.

use crate::object_variant::ObjectVariant;
use crate::string_kind::StringKind;
use std::borrow::Borrow;
use std::sync::Arc;

/// Stores a PDF byte string and its source representation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfString {
    pub bytes: Arc<[u8]>,
    pub kind: StringKind,
}

impl Borrow<[u8]> for PdfString {
    fn borrow(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl AsRef<[u8]> for PdfString {
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl PdfString {
    /// Creates a shared PDF string from bytes and source syntax metadata.
    pub fn from(bytes: impl AsRef<[u8]>, kind: StringKind) -> ObjectVariant {
        ObjectVariant::String(Self::from_bytes(bytes, kind))
    }

    /// Creates a shared string value without wrapping it in an object variant.
    pub(crate) fn from_bytes(bytes: impl AsRef<[u8]>, kind: StringKind) -> Self {
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
