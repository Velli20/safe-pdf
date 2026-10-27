//! PDF names.

use std::borrow::Borrow;
use std::fmt;
use std::ops::Deref;

use crate::inline_bytes::InlineBytes;

/// Stores a PDF name as immutable bytes.
///
/// Names of up to 22 bytes, which covers nearly every dictionary key and name
/// value in practice, are stored inline without a heap allocation. Longer
/// names share one immutable allocation between clones.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PdfName(InlineBytes);

impl PdfName {
    /// Copies a byte sequence into a new PDF name.
    pub fn from_bytes(bytes: impl AsRef<[u8]>) -> Self {
        Self(InlineBytes::from_bytes(bytes))
    }

    /// Returns the decoded bytes of the name without a leading slash.
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

impl From<InlineBytes> for PdfName {
    fn from(bytes: InlineBytes) -> Self {
        Self(bytes)
    }
}

impl From<PdfName> for InlineBytes {
    fn from(name: PdfName) -> Self {
        name.0
    }
}

impl From<&[u8]> for PdfName {
    fn from(bytes: &[u8]) -> Self {
        Self::from_bytes(bytes)
    }
}

impl<const N: usize> From<&[u8; N]> for PdfName {
    fn from(bytes: &[u8; N]) -> Self {
        Self::from_bytes(bytes)
    }
}

impl From<Vec<u8>> for PdfName {
    fn from(bytes: Vec<u8>) -> Self {
        Self::from_bytes(bytes)
    }
}

impl Deref for PdfName {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl Borrow<[u8]> for PdfName {
    fn borrow(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl AsRef<[u8]> for PdfName {
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl fmt::Debug for PdfName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "/{}", self.as_bytes().escape_ascii())
    }
}
