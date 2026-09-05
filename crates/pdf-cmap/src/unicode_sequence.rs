//! Compact Unicode sequences used by PDF character maps.

use std::sync::Arc;

/// Unicode scalars represented by one decoded PDF character code.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum UnicodeSequence {
    /// The PDF code has no usable Unicode mapping.
    #[default]
    Empty,
    /// The PDF code maps to one Unicode scalar.
    Scalar(char),
    /// The PDF code expands to multiple Unicode scalars.
    Sequence(Arc<[char]>),
}

impl UnicodeSequence {
    /// Creates the most compact representation of `characters`.
    #[must_use]
    pub fn from_shared(characters: Arc<[char]>) -> Self {
        match characters.as_ref() {
            [] => Self::Empty,
            [character] => Self::Scalar(*character),
            _ => Self::Sequence(characters),
        }
    }

    /// Borrows the represented Unicode scalars.
    #[must_use]
    pub fn as_slice(&self) -> &[char] {
        match self {
            Self::Empty => &[],
            Self::Scalar(character) => std::slice::from_ref(character),
            Self::Sequence(characters) => characters.as_ref(),
        }
    }
}

impl From<char> for UnicodeSequence {
    fn from(character: char) -> Self {
        Self::Scalar(character)
    }
}

impl From<Arc<[char]>> for UnicodeSequence {
    fn from(characters: Arc<[char]>) -> Self {
        Self::from_shared(characters)
    }
}

impl From<Vec<char>> for UnicodeSequence {
    fn from(characters: Vec<char>) -> Self {
        match characters.as_slice() {
            [] => Self::Empty,
            [character] => Self::Scalar(*character),
            _ => Self::Sequence(Arc::from(characters)),
        }
    }
}
