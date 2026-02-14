use std::collections::BTreeMap;

use crate::{error::ObjectError, object_variant::ObjectVariant};

#[derive(Debug, PartialEq, Clone)]
pub struct Dictionary {
    pub dictionary: BTreeMap<String, ObjectVariant>,
    pub object_number: usize,
}

impl Dictionary {
    pub fn new(dictionary: BTreeMap<String, ObjectVariant>) -> Self {
        Dictionary {
            dictionary,
            object_number: 0,
        }
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
    pub fn get(&self, key: &str) -> Option<&ObjectVariant> {
        self.dictionary.get(key)
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
    pub fn take(&mut self, key: &str) -> Option<ObjectVariant> {
        self.dictionary.remove(key)
    }

    /// Returns a reference to the value associated with the given key, or an error if the key is missing.
    ///
    /// # Parameters
    ///
    /// - `key`: The dictionary entry name to look up.
    ///
    /// # Returns
    ///
    /// Returns `Ok(&ObjectVariant)` if the key exists, or an [`ObjectError::MissingRequiredKey`] if it does not.
    pub fn get_or_err(&self, key: &str) -> Result<&ObjectVariant, ObjectError> {
        self.get(key)
            .ok_or_else(|| ObjectError::MissingRequiredKey {
                key: key.to_string(),
            })
    }
}
