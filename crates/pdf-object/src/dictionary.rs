use std::{collections::BTreeMap, rc::Rc};

use crate::{ObjectVariant, error::ObjectError};

#[derive(Debug, PartialEq, Clone)]
pub struct Dictionary {
    pub dictionary: BTreeMap<String, Box<ObjectVariant>>,
}

impl Dictionary {
    pub fn new(dictionary: BTreeMap<String, Box<ObjectVariant>>) -> Self {
        Dictionary { dictionary }
    }

    /// Returns a reference to the value associated with the given key, if present.
    ///
    /// Parameters:
    /// - `key`: The dictionary entry name to look up.
    ///
    /// Returns `Some(&ObjectVariant)` when the key exists, or `None` if it does not.
    pub fn get(&self, key: &str) -> Option<&ObjectVariant> {
        self.dictionary.get(key).map(|b| b.as_ref())
    }

    /// Returns a reference to the value for `key`, or an error if the key is missing.
    ///
    /// This is a convenience for required entries where absence should be treated as an error.
    ///
    /// Errors
    /// - `ObjectError::MissingRequiredKey` if the key is not found in the dictionary.
    pub fn get_or_err(&self, key: &str) -> Result<&ObjectVariant, ObjectError> {
        self.get(key)
            .ok_or_else(|| ObjectError::MissingRequiredKey {
                key: key.to_string(),
            })
    }

    pub fn get_string(&self, key: &str) -> Option<&str> {
        self.dictionary
            .get(key)
            .and_then(|value| match value.as_ref() {
                ObjectVariant::Name(name) => Some(name.as_ref()),
                _ => None,
            })
    }

    pub fn get_dictionary(&self, key: &str) -> Option<&Rc<Dictionary>> {
        self.dictionary
            .get(key)
            .and_then(|value| match value.as_ref() {
                ObjectVariant::Dictionary(obj) => Some(obj),
                _ => None,
            })
    }
}
