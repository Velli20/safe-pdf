use pdf_object::{
    dictionary::Dictionary,
    error::ObjectError,
    object_collection::ObjectCollection,
    stream::StreamObject,
    traits::{FromDictionary, FromStreamObject},
};
use thiserror::Error;

use crate::{
    character_map::{CMapError, CharacterMap},
    font::FontEncoding,
    font_descriptor::{FontDescriptor, FontDescriptorError},
    simple_font_glyph_map::{SimpleFontGlyphWidthsMap, SimpleFontGlyphWidthsMapError},
};

/// Minimal, initial representation of a PDF TrueType (simple) font.
///
/// Similar to Type1 parsing logic we capture dictionary level metadata
/// required for basic width metrics and embedded program access. Complex
/// encoding differences, glyph substitution, etc. are deferred.
pub struct TrueTypeFont {
    /// PostScript base font name (e.g., /ArialMT)
    pub base_font: String,
    /// Optional font file containing embedded TrueType program.
    pub font_file: Option<StreamObject>,
    /// Widths for character codes 0..=255 if provided via /Widths.
    pub widths: SimpleFontGlyphWidthsMap,
    /// A stream defining a CMap that maps character codes to Unicode values.
    pub cmap: Option<CharacterMap>,
    /// Optional encoding information for simple fonts (Type1, TrueType).
    pub encoding: Option<FontEncoding>,
}

#[derive(Debug, Error, PartialEq)]
pub enum TrueTypeFontError {
    #[error("{0}")]
    ObjectError(#[from] ObjectError),
    #[error("FontDescriptor error: {0}")]
    FontDescriptor(#[from] FontDescriptorError),
    #[error("CMap parsing error: {0}")]
    CMapParse(#[from] CMapError),
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
        let base_font = dictionary
            .get("BaseFont")
            .and_then(|v| v.as_str().map(|s| s.into_owned()))
            .unwrap_or_default();

        // Descriptor is optional for the 14 standard fonts; attempt to resolve if present.
        let font_file = if let Some(fd_obj) = dictionary.get("FontDescriptor") {
            let fd_dict = objects.resolve_dictionary(fd_obj)?;
            let FontDescriptor { font_file } = FontDescriptor::from_dictionary(fd_dict, objects)?;
            font_file
        } else {
            None
        };

        // Attempt to resolve the optional `/ToUnicode` CMap stream, which maps character codes to Unicode.
        // If present, parse it into a `CharacterMap`. If not present, set cmap to None.
        let cmap = dictionary
            .get("ToUnicode")
            .map(|obj| objects.resolve_stream(obj))
            .transpose()?
            .map(CharacterMap::from_stream_object)
            .transpose()?;

        // Attempt to extract the `/Encoding` entry, if present, and convert it to a `FontEncoding`.
        let encoding = dictionary
            .get("Encoding")
            .and_then(|v| v.as_str())
            .map(FontEncoding::from);

        // Read the `/Widths` entry.
        let widths = SimpleFontGlyphWidthsMap::from_dictionary(dictionary, objects)?;

        Ok(Self {
            base_font,
            font_file,
            widths,
            cmap,
            encoding,
        })
    }
}
