/// Stack-allocated sequence of Unicode scalar values for a single PDF character code.
///
/// Capacity of 8 covers all realistic PDF ligatures (2–4 chars); never heap-allocates.
/// An empty `CharVec` is returned when no Unicode mapping is found.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CharVec {
    data: [char; 8],
    len: u8,
}

impl CharVec {
    /// Creates a new empty `CharVec`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a character. Silently drops the character if capacity (8) is exceeded.
    pub fn push(&mut self, c: char) {
        if let Some(slot) = self.data.get_mut(usize::from(self.len)) {
            *slot = c;
            self.len = self.len.saturating_add(1);
        }
    }

    /// Creates a `CharVec` from a slice of characters.
    pub fn from_slice(s: &[char]) -> Self {
        let mut v = Self::new();
        for &c in s {
            v.push(c);
        }
        v
    }
}

impl From<char> for CharVec {
    fn from(c: char) -> Self {
        let mut v = Self::new();
        v.push(c);
        v
    }
}

impl core::ops::Deref for CharVec {
    type Target = [char];

    fn deref(&self) -> &[char] {
        self.data.get(..usize::from(self.len)).unwrap_or(&[])
    }
}

impl<'a> IntoIterator for &'a CharVec {
    type Item = &'a char;
    type IntoIter = core::slice::Iter<'a, char>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty() {
        let v = CharVec::new();
        assert!(v.is_empty());
        assert_eq!(v.len(), 0);
    }

    #[test]
    fn test_push_and_deref() {
        let mut v = CharVec::new();
        v.push('a');
        v.push('b');
        assert_eq!(&*v, ['a', 'b'].as_slice());
    }

    #[test]
    fn test_from_char() {
        let v = CharVec::from('x');
        assert_eq!(&*v, ['x'].as_slice());
    }

    #[test]
    fn test_from_slice() {
        let v = CharVec::from_slice(&['f', 'i']);
        assert_eq!(&*v, ['f', 'i'].as_slice());
    }

    #[test]
    fn test_capacity_overflow_is_silent() {
        let mut v = CharVec::new();
        for _ in 0..10 {
            v.push('z');
        }
        assert_eq!(v.len(), 8);
    }

    #[test]
    fn test_into_iter() {
        let v = CharVec::from_slice(&['a', 'b', 'c']);
        let collected: Vec<char> = v.into_iter().copied().collect();
        assert_eq!(collected, vec!['a', 'b', 'c']);
    }
}
