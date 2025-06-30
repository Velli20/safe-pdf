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
    #[error("Missing /FontDescriptor entry in CIDFont dictionary")]
    MissingFontDescriptor,
    #[error("FontDescriptor parsing error: {0}")]
    FontDescriptorError(#[from] FontDescriptorError),
    #[error("{0}")]
    ObjectError(#[from] ObjectError),
    #[error("GlyphWidthsMap parsing error: {0}")]
    GlyphWidthsMapError(#[from] GlyphWidthsMapError),
    #[error("Missing /Subtype entry in CIDFont dictionary")]
    MissingSubType,
    #[error(
        "Invalid /FontDescriptor reference in CIDFont dictionary: object {0} could not be resolved to a dictionary"
    )]
    InvalidFontDescriptorReference(i32),
    #[error(
        "Invalid type for CIDFont entry /{entry_name}: expected {expected_type}, found {found_type}"
    )]
    InvalidEntryType {
        entry_name: &'static str,
        expected_type: &'static str,
        found_type: &'static str,
    },
    #[error(
        "Unsupported CIDFont subtype '{subtype}': Only /CIDFontType2 (TrueType-based) is supported. /CIDFontType0 (Type1-based) is not supported."
    )]
    UnsupportedCidFontSubtype { subtype: String },
    #[error("Failed to convert PDF value to number for '{entry_description}': {source}")]
    NumericConversionError {
        entry_description: &'static str,
        #[source]
        source: ObjectError,
    },
}

impl FromDictionary for CharacterIdentifierFont {
    const KEY: &'static str = "Font";

    type ResultType = Self;
    type ErrorType = CidFontError;

    fn from_dictionary(
        dictionary: &Dictionary,
        objects: &ObjectCollection,
    ) -> Result<Self::ResultType, Self::ErrorType> {
        let default_width = if let Some(default_width) = dictionary.get("DW") {
            default_width.as_number::<f32>().or_else(|err| {
                Err(CidFontError::NumericConversionError {
                    entry_description: "DW",
                    source: err,
                })
            })?
        } else {
            Self::DEFAULT_WIDTH
        };

        let widths_map = dictionary
            .get_array("W")
            .map(|array| GlyphWidthsMap::from_array(array))
            .transpose()?;

        let subtype = dictionary
            .get_string("Subtype")
            .ok_or(CidFontError::MissingSubType)?;

        // CIDFont subtypes can be /CIDFontType0 or /CIDFontType2.
        // This parser currently only supports /CIDFontType2 (TrueType-based).
        if subtype != "CIDFontType2" {
            return Err(CidFontError::UnsupportedCidFontSubtype {
                subtype: subtype.to_string(),
            });
        }

        // FontDescriptor must be an indirect reference according to the PDF spec.
        let descriptor = if let Some(obj) = dictionary.get("FontDescriptor") {
            let desc_dict = objects.resolve_dictionary(obj.as_ref())?;
            // TODO: Err
            FontDescriptor::from_dictionary(desc_dict, objects)
                .map_err(CidFontError::FontDescriptorError)?
        } else {
            return Err(CidFontError::MissingFontDescriptor);
        };

        Ok(Self {
            default_width,
            descriptor,
            widths: widths_map,
        })
    }
}
