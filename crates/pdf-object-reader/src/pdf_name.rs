//! PDF names.

use std::borrow::Borrow;
use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

/// The longest name stored without a heap allocation.
///
/// With the length byte and the enum tag this keeps a name the size of a `Vec`.
const INLINE_CAPACITY: usize = 22;

/// The storage of a [`PdfName`]: short names live inline, longer ones are shared.
#[derive(Clone)]
enum NameStorage {
    Inline {
        len: u8,
        bytes: [u8; INLINE_CAPACITY],
    },
    Shared(Arc<[u8]>),
}

/// Stores a PDF name as immutable bytes.
///
/// Names of up to 22 bytes, which covers nearly every dictionary key and name
/// value in practice, are stored inline without a heap allocation. Longer
/// names share one immutable allocation between clones.
#[derive(Clone)]
pub struct PdfName(NameStorage);

impl PdfName {
    /// Copies a byte sequence into a new PDF name.
    pub fn from_bytes(bytes: impl AsRef<[u8]>) -> Self {
        let bytes = bytes.as_ref();
        let mut inline = [0; INLINE_CAPACITY];
        match (u8::try_from(bytes.len()), inline.get_mut(..bytes.len())) {
            (Ok(len), Some(prefix)) => {
                prefix.copy_from_slice(bytes);
                Self(NameStorage::Inline { len, bytes: inline })
            }
            _ => Self(NameStorage::Shared(Arc::from(bytes))),
        }
    }

    /// Returns the decoded bytes of the name without a leading slash.
    pub fn as_bytes(&self) -> &[u8] {
        match &self.0 {
            NameStorage::Inline { len, bytes } => bytes.get(..usize::from(*len)).unwrap_or(&[]),
            NameStorage::Shared(bytes) => bytes,
        }
    }

    /// Shares the bytes of an existing allocation, copying them inline when they are short.
    pub(crate) fn from_shared(bytes: &Arc<[u8]>) -> Self {
        if bytes.len() <= INLINE_CAPACITY {
            Self::from_bytes(bytes.as_ref())
        } else {
            Self(NameStorage::Shared(Arc::clone(bytes)))
        }
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

impl PartialEq for PdfName {
    fn eq(&self, other: &Self) -> bool {
        self.as_bytes() == other.as_bytes()
    }
}

impl Eq for PdfName {}

impl PartialOrd for PdfName {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PdfName {
    fn cmp(&self, other: &Self) -> Ordering {
        self.as_bytes().cmp(other.as_bytes())
    }
}

impl Hash for PdfName {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_bytes().hash(state);
    }
}

impl fmt::Debug for PdfName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "/{}", self.as_bytes().escape_ascii())
    }
}
