use crate::{object_id::ObjectId, object_variant::ObjectVariant, pdf_name::PdfName};

/// Stores an immutable PDF dictionary.
///
/// Entries are kept sorted by key with each key present once.
#[derive(Debug, PartialEq, Clone, Default)]
pub struct Dictionary {
    entries: Vec<(PdfName, ObjectVariant)>,
    pub object_number: Option<usize>,
}

impl Dictionary {
    /// Creates a hand-built dictionary from borrowed PDF Name keys.
    pub fn from_entries<const N: usize>(entries: [(&[u8], ObjectVariant); N]) -> Self {
        entries.into_iter().collect()
    }

    /// Returns a reference to the value associated with the given key, if present.
    ///
    /// # Parameters:
    ///
    ///  - `key`: The dictionary entry name to look up.
    ///
    /// # Returns
    ///
    /// Returns an optional reference to [`ObjectVariant`] when the key exists, or `None` if it does not.
    pub fn get(&self, key: &[u8]) -> Option<&ObjectVariant> {
        let index = self.position(key).ok()?;
        self.entries.get(index).map(|(_, value)| value)
    }

    /// Iterates the entries in key order, yielding each PDF Name key as a byte slice.
    pub fn iter(&self) -> impl Iterator<Item = (&[u8], &ObjectVariant)> {
        self.entries
            .iter()
            .map(|(key, value)| (key.as_bytes(), value))
    }

    /// Iterates the entries in key order with mutable access to each value.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&[u8], &mut ObjectVariant)> {
        self.entries
            .iter_mut()
            .map(|(key, value)| (key.as_bytes(), value))
    }

    /// Iterates the entry names in key order.
    pub fn keys(&self) -> impl Iterator<Item = &[u8]> {
        self.entries.iter().map(|(key, _)| key.as_bytes())
    }

    /// Returns the number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` when the dictionary has no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Removes and returns the value associated with the given key, if present.
    ///
    /// # Parameters
    ///
    /// - `key`: The dictionary entry name to remove and return.
    ///
    /// # Returns
    ///
    /// Returns an `Option` containing the [`ObjectVariant`] if the key exists, or `None` if it does not.
    pub fn take(&mut self, key: &[u8]) -> Option<ObjectVariant> {
        let index = self.position(key).ok()?;
        (index < self.entries.len()).then(|| self.entries.remove(index).1)
    }

    /// Sets the value of `key`, returning the value it replaces, if any.
    pub fn insert(
        &mut self,
        key: impl Into<PdfName>,
        value: ObjectVariant,
    ) -> Option<ObjectVariant> {
        let key = key.into();
        match self.position(key.as_bytes()) {
            Ok(index) => self
                .entries
                .get_mut(index)
                .map(|(_, existing)| std::mem::replace(existing, value)),
            Err(index) => {
                self.entries.insert(index, (key, value));
                None
            }
        }
    }

    /// Returns the `/Parent` entry and its object id when it is an indirect reference.
    pub fn parent_reference(&self) -> Option<(&ObjectVariant, ObjectId)> {
        match self.get(b"Parent") {
            Some(parent @ ObjectVariant::Reference(parent_id)) => Some((parent, *parent_id)),
            _ => None,
        }
    }

    /// Finds the index of `key`, or where it would be inserted.
    #[inline]
    fn position(&self, key: &[u8]) -> Result<usize, usize> {
        self.entries
            .binary_search_by(|(name, _)| name.as_bytes().cmp(key))
    }
}

/// Collects entries in source order; when a key repeats, the later definition replaces
/// the earlier one.
impl<K: Into<PdfName>> FromIterator<(K, ObjectVariant)> for Dictionary {
    fn from_iter<T: IntoIterator<Item = (K, ObjectVariant)>>(entries: T) -> Self {
        let mut dictionary = Self::default();
        for (key, value) in entries {
            dictionary.insert(key, value);
        }
        dictionary
    }
}

impl IntoIterator for Dictionary {
    type Item = (PdfName, ObjectVariant);
    type IntoIter = std::vec::IntoIter<(PdfName, ObjectVariant)>;

    /// Moves the entries out in key order.
    fn into_iter(self) -> Self::IntoIter {
        self.entries.into_iter()
    }
}
