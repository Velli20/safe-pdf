use crate::{dictionary::Dictionary, object_collection::ObjectCollection, stream::StreamObject};

/// A trait for types that can be constructed from a PDF [`Dictionary`].
///
/// This trait is used to abstract the process of extracting and parsing
/// specific entries from a PDF dictionary, potentially resolving indirect
/// objects using an [`ObjectCollection`].
pub trait FromDictionary {
    /// The key in the PDF dictionary that this type is responsible for parsing.
    /// For example, for a `MediaBox`, this would be "MediaBox".
    const KEY: &'static str;

    /// The type that will be produced after successfully parsing the dictionary entry.
    type ResultType;

    /// The type of error that can occur during parsing.
    type ErrorType;

    /// Attempts to construct an instance of `Self::ResultType` from the given PDF dictionary.
    ///
    /// # Arguments
    ///
    /// - `dictionary`: A reference to the PDF [`Dictionary`] to parse.
    /// - `objects`: A reference to the [`ObjectCollection`] used to resolve any indirect objects.
    ///
    /// # Returns
    ///
    /// A `Result` containing the parsed `Self::ResultType` on success, or a [`ErrorType`] on failure.
    fn from_dictionary(
        dictionary: &Dictionary,
        objects: &ObjectCollection,
    ) -> Result<Self::ResultType, Self::ErrorType>;
}

/// A trait for types that can be constructed from a PDF [`StreamObject`].
pub trait FromStreamObject {
    /// The type that will be produced after successfully parsing the dictionary entry.
    type ResultType;

    /// The type of error that can occur during parsing.
    type ErrorType;

    /// Attempts to construct an instance of `Self::ResultType` from the given PDF stream object.
    ///
    /// # Arguments
    ///
    /// - `stream`: A reference to the PDF [`StreamObject`] to parse.
    ///
    /// # Returns
    ///
    /// A `Result` containing the parsed `Self::ResultType` on success, or a [`ErrorType`] on failure.
    fn from_stream_object(stream: &StreamObject) -> Result<Self::ResultType, Self::ErrorType>;
}
