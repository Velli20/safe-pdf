/// Represents an error that can occur while handling objects.
#[derive(Debug)]
pub enum ObjectError {
    /// Indicates that an attempt was made to insert an object into an `ObjectCollection`
    /// with an object number that already exists in the collection.
    DuplicateKeyInObjectCollection(i32),
    /// Indicates a mismatch between the expected type and the actual type of a `Value`.
    /// This can occur when trying to interpret a `Value` as a specific concrete type.
    TypeMismatch(&'static str, &'static str),
    /// Indicates an error occurred while attempting to convert a `Value::Number`
    /// to a different numeric type (e.g., when `TryFrom` fails).
    NumberConversionError,
}

impl std::fmt::Display for ObjectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ObjectError::DuplicateKeyInObjectCollection(key) => {
                write!(
                    f,
                    "Object with the given key {} already exists in collection",
                    key
                )
            }

            ObjectError::TypeMismatch(expected, actual) => {
                write!(
                    f,
                    "Type mismatch: expected type '{}', but found type '{}'",
                    expected, actual
                )
            }
            ObjectError::NumberConversionError => {
                write!(f, "Failed to convert number to the requested type",)
            }
        }
    }
}
