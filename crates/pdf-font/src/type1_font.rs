use pdf_object::{
    ObjectVariant, dictionary::Dictionary, error::ObjectError, object_collection::ObjectCollection,
    traits::FromDictionary,
};
use thiserror::Error;

use crate::{
    cff_builder::build_cff_font,
    encoding::{Encoding, FontEncoding},
    font::FontError,
    font_descriptor::{FontDescriptor, FontDescriptorError},
    simple_font_glyph_map::{SimpleFontGlyphWidthsMap, SimpleFontGlyphWidthsMapError},
};

/// Minimal, initial representation of a PDF Type1 font.
///
/// This focuses on dictionary-level metadata needed by higher layers
/// and defers actual glyph rendering or embedded program parsing.
pub struct Type1Font {
    /// A stream containing the font program.
    pub font_file: Vec<u8>,
    /// Widths map for character codes.
    pub widths: SimpleFontGlyphWidthsMap,
    /// Optional encoding information.
    pub encoding: Encoding,
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
    type ErrorType = FontError;

    fn from_dictionary(
        dictionary: &Dictionary,
        objects: &ObjectCollection,
    ) -> Result<Self::ResultType, Self::ErrorType> {
        // Read '/FontDescriptor’.
        let descriptor = objects.resolve_dictionary(dictionary.get_or_err("FontDescriptor")?)?;

        let font_file = FontDescriptor::from_dictionary(descriptor, objects)?;

        let font_file = font_file.data.as_slice();
        let font_file = build_cff_font(font_file)?;

        // Read the `/Widths` entry.
        let widths = SimpleFontGlyphWidthsMap::from_dictionary(dictionary, objects)?;

        // TODO: Handle `/FontMatrix`.

        // Read optional `/Encoding` entry. This is either a name or a dictionary.
        let encoding = if let Some(enc_obj) = dictionary.get("Encoding") {
            let enc_obj = objects.resolve_object(enc_obj)?;
            match enc_obj {
                ObjectVariant::Dictionary(enc_dictionary) => {
                    Encoding::from_dictionary(enc_dictionary, objects)?
                }
                _ => {
                    let base = FontEncoding::from(enc_obj.try_str()?);
                    Encoding::from_base_encoding(base)?
                }
            }
        } else {
            Encoding::default()
        };

        Ok(Self {
            font_file,
            widths,
            encoding,
        })
    }
}
