use pdf_object::{
    dictionary::Dictionary, error::ObjectError, object_collection::ObjectCollection,
    stream::StreamObject, traits::FromDictionary,
};
use thiserror::Error;

use crate::{
    font_descriptor::{FontDescriptor, FontDescriptorError},
    simple_font_glyph_map::{SimpleFontGlyphWidthsMap, SimpleFontGlyphWidthsMapError},
};

/// Minimal, initial representation of a PDF Type1 font.
///
/// This focuses on dictionary-level metadata needed by higher layers
/// and defers actual glyph rendering or embedded program parsing.
pub struct Type1Font {
    /// PostScript base font name (e.g., /Helvetica)
    pub base_font: String,
    /// A stream containing the font program.
    pub font_file: Option<StreamObject>,
    /// Optional encoding name (e.g., /WinAnsiEncoding) or custom encoding via Differences
    /// For now we capture only the base encoding name for quick wiring; differences can be
    /// expanded later similarly to Type3.
    pub base_encoding: Option<String>,
    /// Widths map for character codes.
    pub widths: SimpleFontGlyphWidthsMap,
}

/// Errors that can occur while parsing a Type1 font dictionary.
#[derive(Debug, Error, PartialEq)]
pub enum Type1FontError {
    #[error("{0}")]
    ObjectError(#[from] ObjectError),
    #[error("FontDescriptor error: {0}")]
    FontDescriptor(#[from] FontDescriptorError),
    #[error("SimpleFontGlyphWidthsMap parsing error: {0}")]
    SimpleFontGlyphWidthsMapError(#[from] SimpleFontGlyphWidthsMapError),
}

impl FromDictionary for Type1Font {
    const KEY: &'static str = "Font";
    type ResultType = Self;
    type ErrorType = Type1FontError;

    fn from_dictionary(
        dictionary: &Dictionary,
        objects: &ObjectCollection,
    ) -> Result<Self::ResultType, Self::ErrorType> {
        // BaseFont is recommended for Type1. Default to empty string if missing.
        let base_font = dictionary
            .get("BaseFont")
            .and_then(|v| v.as_str().map(|s| s.into_owned()))
            .unwrap_or_default();

        // Read '/FontDescriptor’.
        let fd = dictionary.get_or_err("FontDescriptor")?;
        let FontDescriptor { font_file } =
            FontDescriptor::from_dictionary(objects.resolve_dictionary(fd)?, objects)?;

        // Encoding may be a name or a dictionary. For initial support, record only base name.
        let base_encoding = dictionary
            .get("Encoding")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // Read the `/Widths` entry.
        let widths = SimpleFontGlyphWidthsMap::from_dictionary(dictionary, objects)?;

        Ok(Self {
            base_font,
            font_file,
            base_encoding,
            widths,
        })
    }
}
