//! PDF names.

use std::borrow::Borrow;
use std::sync::Arc;

/// Stores a PDF name as immutable shared bytes.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PdfName(Arc<[u8]>);

impl PdfName {
    /// Copies a byte sequence into a new PDF name.
    pub fn from_bytes(bytes: impl AsRef<[u8]>) -> Self {
        Self(Arc::from(bytes.as_ref()))
    }

    /// Returns the decoded bytes of the name without a leading slash.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl Borrow<[u8]> for PdfName {
    fn borrow(&self) -> &[u8] {
        self.as_bytes()
    }
}
