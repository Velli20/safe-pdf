use std::collections::BTreeMap;

use crate::{object_id::ObjectId, object_variant::ObjectVariant};

#[derive(Debug, PartialEq, Clone)]
pub struct Dictionary {
    pub dictionary: BTreeMap<Vec<u8>, ObjectVariant>,
    pub object_number: Option<usize>,
}

impl Dictionary {
    /// Creates a dictionary from already-owned PDF Name keys.
    pub fn new(dictionary: BTreeMap<Vec<u8>, ObjectVariant>) -> Self {
        Dictionary {
            dictionary,
            object_number: None,
        }
    }

    /// Creates a hand-built dictionary from borrowed PDF Name keys.
    ///
    /// This keeps the unavoidable ownership copy at the dictionary boundary.
    pub fn from_entries<const N: usize>(entries: [(&[u8], ObjectVariant); N]) -> Self {
        Self::new(
            entries
                .into_iter()
                .map(|(key, value)| (Vec::from(key), value))
                .collect(),
        )
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
        self.dictionary.get(key)
    }

    /// Iterates the entries in key order, yielding each PDF Name key as a byte slice.
    pub fn iter(&self) -> impl Iterator<Item = (&[u8], &ObjectVariant)> {
        self.dictionary
            .iter()
            .map(|(key, value)| (key.as_slice(), value))
    }

    /// Returns the number of entries.
    pub fn len(&self) -> usize {
        self.dictionary.len()
    }

    /// Returns `true` when the dictionary has no entries.
    pub fn is_empty(&self) -> bool {
        self.dictionary.is_empty()
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
        self.dictionary.remove(key)
    }

    /// Returns the `/Parent` entry and its object id when it is an indirect reference.
    pub fn parent_reference(&self) -> Option<(&ObjectVariant, ObjectId)> {
        match self.get(b"Parent") {
            Some(parent @ ObjectVariant::Reference(parent_id)) => Some((parent, *parent_id)),
            _ => None,
        }
    }
}
