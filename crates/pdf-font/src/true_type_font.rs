use pdf_object::{
    dictionary::Dictionary, error::ObjectError, object_collection::ObjectCollection,
    stream::StreamObject, traits::FromDictionary,
};
use thiserror::Error;

use crate::{
    font_descriptor::{FontDescriptor, FontDescriptorError},
    simple_font_glyph_map::{SimpleFontGlyphWidthsMap, SimpleFontGlyphWidthsMapError},
};

/// Minimal, initial representation of a PDF TrueType (simple) font.
pub struct TrueTypeFont {
    /// Optional font file containing embedded TrueType program.
    pub font_file: StreamObject,
    /// Widths for character codes 0..=255 if provided via /Widths.
    pub widths: SimpleFontGlyphWidthsMap,
}

#[derive(Debug, Error, PartialEq)]
pub enum TrueTypeFontError {
    #[error("{0}")]
    ObjectError(#[from] ObjectError),
    #[error("FontDescriptor error: {0}")]
    FontDescriptor(#[from] FontDescriptorError),
    #[error("SimpleFontGlyphWidthsMap parsing error: {0}")]
    SimpleFontGlyphWidthsMapError(#[from] SimpleFontGlyphWidthsMapError),
}

impl FromDictionary for TrueTypeFont {
    const KEY: &'static str = "Font";
    type ResultType = Self;
    type ErrorType = TrueTypeFontError;

    fn from_dictionary(
        dictionary: &Dictionary,
        objects: &ObjectCollection,
    ) -> Result<Self::ResultType, Self::ErrorType> {
        // Read the `/FontDescriptor` entry.
        let font_descriptor = dictionary.get_or_err("FontDescriptor")?;
        let font_file =
            FontDescriptor::from_dictionary(objects.resolve_dictionary(font_descriptor)?, objects)?;

        // Read the `/Widths` entry.
        let widths = SimpleFontGlyphWidthsMap::from_dictionary(dictionary, objects)?;

        Ok(Self { font_file, widths })
    }
}
