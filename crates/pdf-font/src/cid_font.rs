use pdf_object::{
    dictionary::Dictionary, error::ObjectError, object_collection::ObjectCollection,
    traits::FromDictionary,
};

use crate::{
    font_descriptor::{FontDescriptor, FontDescriptorError},
    glyph_widths_map::{GlyphWidthsMap, GlyphWidthsMapError},
};
use thiserror::Error;

/// Represents a Character Identifier (CID) font.
///
/// CID-keyed fonts are a sophisticated type of font that can handle large character sets,
/// such as those required for East Asian languages. They define glyphs by character identifiers (CIDs)
/// rather than by character codes, providing a flexible way to manage complex typography.
pub struct CharacterIdentifierFont {
    /// The default width for glyphs in the font.
    /// This is the `/DW` entry in the CIDFont dictionary.
    pub default_width: f32,
    /// The font descriptor for this font, providing detailed metrics and style information.
    pub descriptor: FontDescriptor,
    /// A map of individual glyph widths, overriding the default width for specific CIDs.
    /// This corresponds to the `/W` entry in the CIDFont dictionary.
    pub widths: Option<GlyphWidthsMap>,
}

impl CharacterIdentifierFont {
    /// Default value for the `/DW` entry, if not present in the font dictionary.
    const DEFAULT_WIDTH: f32 = 1000.0;
}

/// Defines errors that can occur while reading a PDF objects.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum CidFontError {
    #[error("FontDescriptor parsing error: {0}")]
    FontDescriptorError(#[from] FontDescriptorError),
    #[error("{0}")]
    ObjectError(#[from] ObjectError),
    #[error("GlyphWidthsMap parsing error: {0}")]
    GlyphWidthsMapError(#[from] GlyphWidthsMapError),
    #[error(
        "Unsupported CIDFont subtype '{subtype}': Only /CIDFontType2 (TrueType-based) is supported. /CIDFontType0 (Type1-based) is not supported."
    )]
    UnsupportedCidFontSubtype { subtype: String },
}

impl FromDictionary for CharacterIdentifierFont {
    const KEY: &'static str = "Font";

    type ResultType = Self;
    type ErrorType = CidFontError;

    fn from_dictionary(
        dictionary: &Dictionary,
        objects: &ObjectCollection,
    ) -> Result<Self::ResultType, Self::ErrorType> {
        let default_width = dictionary
            .get("DW")
            .map(|dw| dw.as_number::<f32>())
            .transpose()?
            .unwrap_or(Self::DEFAULT_WIDTH);

        let widths_map = dictionary
            .get("W")
            .map(|obj| -> Result<GlyphWidthsMap, CidFontError> {
                let arr = obj.try_array()?;
                GlyphWidthsMap::from_array(arr).map_err(CidFontError::from)
            })
            .transpose()?;

        let subtype = dictionary.get_or_err("Subtype")?.try_str()?;

        // CIDFont subtypes can be /CIDFontType0 or /CIDFontType2.
        // This parser currently only supports /CIDFontType2 (TrueType-based).
        if subtype != "CIDFontType2" {
            return Err(CidFontError::UnsupportedCidFontSubtype {
                subtype: subtype.to_string(),
            });
        }

        // FontDescriptor must be an indirect reference according to the PDF spec.
        let desc_dict = objects.resolve_dictionary(dictionary.get_or_err("FontDescriptor")?)?;
        let descriptor = FontDescriptor::from_dictionary(desc_dict, objects)?;

        Ok(Self {
            default_width,
            descriptor,
            widths: widths_map,
        })
    }
}
