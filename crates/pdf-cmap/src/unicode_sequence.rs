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

impl FromIterator<char> for UnicodeSequence {
    /// Collects `characters`, allocating shared storage only for two or more scalars.
    fn from_iter<I: IntoIterator<Item = char>>(characters: I) -> Self {
        let mut characters = characters.into_iter();
        let Some(first) = characters.next() else {
            return Self::Empty;
        };
        let Some(second) = characters.next() else {
            return Self::Scalar(first);
        };
        Self::Sequence([first, second].into_iter().chain(characters).collect())
    }
}
