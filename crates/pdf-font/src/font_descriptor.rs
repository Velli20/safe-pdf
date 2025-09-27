use pdf_object::{
    ObjectVariant, dictionary::Dictionary, error::ObjectError, object_collection::ObjectCollection,
    traits::FromDictionary,
};
use thiserror::Error;

/// Defines errors that can occur while reading or processing font-related PDF objects.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum FontDescriptorError {
    #[error("Font file stream is missing")]
    MissingFontFile,
    #[error("Object error: {0}")]
    ObjectError(#[from] ObjectError),
    #[error("FontName is required but was an empty string")]
    EmptyFontName,
}

/// Represents a font descriptor, a dictionary that provides detailed information
/// about a font, such as its metrics, style, and font file data.
#[derive(Debug)]
pub struct FontDescriptor {
    /// A stream containing the font program.
    /// This can be FontFile, FontFile2, or FontFile3 depending on the font type.
    pub font_file: ObjectVariant,
}

impl FromDictionary for FontDescriptor {
    const KEY: &'static str = "FontDescriptor";

    type ResultType = Self;
    type ErrorType = FontDescriptorError;

    fn from_dictionary(
        dictionary: &Dictionary,
        objects: &ObjectCollection,
    ) -> Result<Self::ResultType, Self::ErrorType> {
        let resolve_font_file_stream = |key: &str| -> Option<ObjectVariant> {
            dictionary.get(key).and_then(|obj| match obj {
                ObjectVariant::Reference(id) => objects.get(*id).cloned(),
                ObjectVariant::Stream(s) => Some(ObjectVariant::Stream(std::rc::Rc::clone(s))),
                _ => None,
            })
        };

        let font_file = resolve_font_file_stream("FontFile2")
            .or_else(|| resolve_font_file_stream("FontFile3"))
            .or_else(|| resolve_font_file_stream("FontFile"));

        let font_file = font_file.ok_or(FontDescriptorError::MissingFontFile)?;
        Ok(Self { font_file })
    }
}
