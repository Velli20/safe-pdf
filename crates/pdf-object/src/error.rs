use thiserror::Error;

/// Represents an error that can occur while handling objects.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum ObjectError {
    /// Indicates that an attempt was made to insert an object into an `ObjectCollection`
    /// with an object number that already exists in the collection.
    #[error("Object with the given key {0} already exists in collection")]
    DuplicateKeyInObjectCollection(i32),
    /// Indicates a mismatch between the expected type and the actual type of a `Value`.
    /// This can occur when trying to interpret a `Value` as a specific concrete type.
    #[error("Type mismatch: expected type '{0}', but found type '{1}'")]
    TypeMismatch(&'static str, &'static str),
    /// Indicates an error occurred while attempting to convert a `Value::Number`
    /// to a different numeric type (e.g., when `TryFrom` fails).
    #[error("Failed to convert number to the requested type")]
    NumberConversionError,
    #[error("Failed to resolve an object reference {obj_num}")]
    FailedResolveObjectReference { obj_num: i32 },
    #[error("Failed to resolve an object to a dictionary, but found type '{resolved_type}'")]
    FailedResolveDictionaryObject { resolved_type: &'static str },
    #[error("Failed to resolve an object to a stream, but found type '{resolved_type}'")]
    FailedResolveStreamObject { resolved_type: &'static str },
}
