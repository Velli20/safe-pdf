//! PDF byte strings.

use crate::inline_bytes::InlineBytes;
use crate::object_variant::ObjectVariant;
use crate::string_kind::StringKind;
use std::borrow::Borrow;

/// Stores a PDF byte string and its source representation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfString {
    pub bytes: InlineBytes,
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
    /// Creates a PDF string from bytes and source syntax metadata.
    pub fn from(bytes: impl AsRef<[u8]>, kind: StringKind) -> ObjectVariant {
        ObjectVariant::String(Self {
            bytes: InlineBytes::from_bytes(bytes),
            kind,
        })
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
