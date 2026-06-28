use std::collections::BTreeMap;

use crate::{error::ObjectError, object_variant::ObjectVariant};

#[derive(Debug, PartialEq, Clone)]
pub struct Dictionary {
    pub dictionary: BTreeMap<String, ObjectVariant>,
    pub object_number: Option<usize>,
}

impl Dictionary {
    pub fn new(dictionary: BTreeMap<String, ObjectVariant>) -> Self {
        Dictionary {
            dictionary,
            object_number: None,
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

    /// Converts the value associated with the given key when it is present.
    ///
    /// # Parameters
    ///
    /// - `key`: The dictionary entry name to look up.
    ///
    /// # Returns
    ///
    /// Returns `Ok(Some(T))` when the key exists and conversion succeeds, `Ok(None)` when the
    /// key is absent, or `Err(T::Error)` when conversion fails.
    pub fn try_get_as<T>(
        &self,
        key: &str,
    ) -> Result<Option<T>, <T as TryFrom<&ObjectVariant>>::Error>
    where
        for<'a> T: TryFrom<&'a ObjectVariant>,
    {
        self.get(key).map(T::try_from).transpose()
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
                key: key.to_owned(),
            })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    #[derive(Debug, PartialEq)]
    struct ParsedInteger(i64);

    impl TryFrom<&ObjectVariant> for ParsedInteger {
        type Error = &'static str;

        fn try_from(value: &ObjectVariant) -> Result<Self, Self::Error> {
            match value {
                ObjectVariant::Integer(value) => Ok(Self(*value)),
                _ => Err("expected integer"),
            }
        }
    }

    fn dictionary_with(key: &str, value: ObjectVariant) -> Dictionary {
        let mut values = BTreeMap::new();
        values.insert(key.to_owned(), value);
        Dictionary::new(values)
    }

    #[test]
    fn try_get_as_converts_existing_value() {
        let dictionary = dictionary_with("Count", ObjectVariant::Integer(42));

        let value = dictionary
            .try_get_as::<ParsedInteger>("Count")
            .expect("integer parses");

        assert_eq!(value, Some(ParsedInteger(42)));
    }

    #[test]
    fn try_get_as_returns_none_for_missing_key() {
        let dictionary = Dictionary::new(BTreeMap::new());

        let value = dictionary
            .try_get_as::<ParsedInteger>("Count")
            .expect("missing key is not an error");

        assert_eq!(value, None);
    }

    #[test]
    fn try_get_as_propagates_conversion_error() {
        let dictionary = dictionary_with("Count", ObjectVariant::Name(b"Count".to_vec()));

        let error = dictionary
            .try_get_as::<ParsedInteger>("Count")
            .expect_err("name is not an integer");

        assert_eq!(error, "expected integer");
    }
}
