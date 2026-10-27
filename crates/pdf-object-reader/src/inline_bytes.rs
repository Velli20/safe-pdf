//! Immutable byte sequences stored inline when short.

use std::borrow::Borrow;
use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::ops::Deref;
use std::sync::Arc;

/// The longest byte sequence stored without a heap allocation.
///
/// With the length byte and the enum tag this keeps the value the size of a `Vec`.
const INLINE_CAPACITY: usize = 22;

/// The storage of [`InlineBytes`]: short sequences live inline, longer ones are shared.
#[derive(Clone)]
enum Storage {
    Inline {
        len: u8,
        bytes: [u8; INLINE_CAPACITY],
    },
    Shared(Arc<[u8]>),
}

/// Stores an immutable byte sequence.
///
/// Sequences of up to 22 bytes, which covers nearly every name and most strings
/// shown by content streams, are stored inline without a heap allocation. Longer
/// sequences share one immutable allocation between clones.
#[derive(Clone)]
pub struct InlineBytes(Storage);

impl InlineBytes {
    /// Copies a byte sequence into new storage.
    pub fn from_bytes(bytes: impl AsRef<[u8]>) -> Self {
        let bytes = bytes.as_ref();
        let mut inline = [0; INLINE_CAPACITY];
        match (u8::try_from(bytes.len()), inline.get_mut(..bytes.len())) {
            (Ok(len), Some(prefix)) => {
                prefix.copy_from_slice(bytes);
                Self(Storage::Inline { len, bytes: inline })
            }
            _ => Self(Storage::Shared(Arc::from(bytes))),
        }
    }

    /// Shares the bytes of an existing allocation, copying them inline when they are short.
    pub fn from_shared(bytes: &Arc<[u8]>) -> Self {
        if bytes.len() <= INLINE_CAPACITY {
            Self::from_bytes(bytes.as_ref())
        } else {
            Self(Storage::Shared(Arc::clone(bytes)))
        }
    }

    /// Returns the stored bytes.
    pub fn as_bytes(&self) -> &[u8] {
        match &self.0 {
            Storage::Inline { len, bytes } => bytes.get(..usize::from(*len)).unwrap_or(&[]),
            Storage::Shared(bytes) => bytes,
        }
    }

    /// Returns the bytes as a shared allocation, allocating only when they are stored inline.
    pub fn to_shared(&self) -> Arc<[u8]> {
        match &self.0 {
            Storage::Inline { .. } => Arc::from(self.as_bytes()),
            Storage::Shared(bytes) => Arc::clone(bytes),
        }
    }
}

impl Default for InlineBytes {
    fn default() -> Self {
        Self(Storage::Inline {
            len: 0,
            bytes: [0; INLINE_CAPACITY],
        })
    }
}

/// Collects bytes inline, moving them to a shared allocation once they outgrow it.
impl FromIterator<u8> for InlineBytes {
    fn from_iter<I: IntoIterator<Item = u8>>(iter: I) -> Self {
        let mut iter = iter.into_iter();
        let mut inline = [0; INLINE_CAPACITY];
        let mut len = 0_u8;
        for slot in &mut inline {
            let Some(byte) = iter.next() else {
                return Self(Storage::Inline { len, bytes: inline });
            };
            *slot = byte;
            len = len.saturating_add(1);
        }
        let Some(next) = iter.next() else {
            return Self(Storage::Inline { len, bytes: inline });
        };
        let mut spilled = Vec::with_capacity(INLINE_CAPACITY.saturating_mul(2));
        spilled.extend_from_slice(&inline);
        spilled.push(next);
        spilled.extend(iter);
        Self(Storage::Shared(Arc::from(spilled)))
    }
}

impl From<&[u8]> for InlineBytes {
    fn from(bytes: &[u8]) -> Self {
        Self::from_bytes(bytes)
    }
}

impl<const N: usize> From<&[u8; N]> for InlineBytes {
    fn from(bytes: &[u8; N]) -> Self {
        Self::from_bytes(bytes)
    }
}

impl From<Vec<u8>> for InlineBytes {
    fn from(bytes: Vec<u8>) -> Self {
        Self::from_bytes(bytes)
    }
}

impl From<Arc<[u8]>> for InlineBytes {
    fn from(bytes: Arc<[u8]>) -> Self {
        Self::from_shared(&bytes)
    }
}

impl Deref for InlineBytes {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl Borrow<[u8]> for InlineBytes {
    fn borrow(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl AsRef<[u8]> for InlineBytes {
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl PartialEq for InlineBytes {
    fn eq(&self, other: &Self) -> bool {
        self.as_bytes() == other.as_bytes()
    }
}

impl Eq for InlineBytes {}

impl PartialOrd for InlineBytes {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for InlineBytes {
    fn cmp(&self, other: &Self) -> Ordering {
        self.as_bytes().cmp(other.as_bytes())
    }
}

impl Hash for InlineBytes {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_bytes().hash(state);
    }
}

impl fmt::Debug for InlineBytes {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_bytes().fmt(formatter)
    }
}
