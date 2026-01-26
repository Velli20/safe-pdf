use pdf_object::{
    dictionary::Dictionary, error::ObjectError, object_collection::ObjectCollection,
    stream::StreamObject, traits::FromDictionary,
};
use thiserror::Error;

/// Defines errors that can occur while reading or processing font-related PDF objects.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum FontDescriptorError {
    #[error("Font file stream is missing")]
    MissingFontFile,
    #[error("Object error: {0}")]
    ObjectError(#[from] ObjectError),
}

/// Represents a font descriptor, a dictionary that provides detailed information
/// about a font, such as its metrics, style, and font file data.
#[derive(Debug)]
pub struct FontDescriptor {
    /// A stream containing the font program.
    pub font_file: Option<StreamObject>,
}

impl FromDictionary for FontDescriptor {
    const KEY: &'static str = "FontDescriptor";

    type ResultType = StreamObject;
    type ErrorType = FontDescriptorError;

    fn from_dictionary(
        dictionary: &Dictionary,
        objects: &ObjectCollection,
    ) -> Result<Self::ResultType, Self::ErrorType> {
        let resolve = |key| {
            dictionary
                .get(key)
                .map(|obj| obj.try_stream(objects))
                .transpose()
        };

        resolve("FontFile2")?
            .or(resolve("FontFile3")?)
            .or(resolve("FontFile")?)
            .cloned()
            .ok_or(FontDescriptorError::MissingFontFile)
    }
}
