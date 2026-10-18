//! Errors decoding optional content declarations.

/// A failure decoding an optional content declaration.
#[derive(Debug, thiserror::Error)]
pub enum OptionalContentError {
    /// An error returned while reading or resolving a PDF object.
    #[error("{0}")]
    Object(#[from] pdf_object_reader::object_error::ObjectError),
    /// An entry is present but has the wrong type or value shape.
    #[error("invalid optional content entry '/{entry:?}': {reason}")]
    InvalidEntry {
        /// The dictionary key of the offending entry.
        entry: &'static [u8],
        /// Why the entry was rejected.
        reason: String,
    },
    /// A required entry is missing.
    #[error("missing required optional content entry '/{entry:?}'")]
    MissingEntry {
        /// The dictionary key of the missing entry.
        entry: &'static [u8],
    },
    /// An object number cannot be represented as a group identity.
    #[error("optional content group object number is out of range")]
    ResourceLimit,
}

impl From<OptionalContentError> for pdf_object_reader::ObjectReadError {
    fn from(source: OptionalContentError) -> Self {
        Self::Decode {
            target: "optional content",
            source: Box::new(source),
        }
    }
}
