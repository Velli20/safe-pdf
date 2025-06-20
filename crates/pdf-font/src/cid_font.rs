use pdf_object::{
    ObjectVariant, dictionary::Dictionary, object_collection::ObjectCollection,
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
    pub default_width: i64,
    /// The font descriptor for this CIDFont, providing detailed metrics and style information.
    pub descriptor: FontDescriptor,
    /// The subtype of this CIDFont, which can be `/CIDFontType0` (for Type 1-based CIDs)
    /// or `/CIDFontType2` (for TrueType-based CIDs).
    subtype: String,

    pub widths: Option<GlyphWidthsMap>,
}

impl CharacterIdentifierFont {
    /// Default value for the `/DW` entry, if not present in the font dictionary.
    const DEFAULT_WIDTH: i64 = 1000;
}

/// Defines errors that can occur while reading a PDF objects.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum CidFontError {
    #[error("Missing /FontDescriptor entry in CIDFont dictionary")]
    MissingFontDescriptor,
    #[error("FontDescriptor parsing error: {0}")]
    FontDescriptorError(#[from] FontDescriptorError),
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
}

impl FromDictionary for CharacterIdentifierFont {
    const KEY: &'static str = "Font";

    type ResultType = Self;
    type ErrorType = CidFontError;

    fn from_dictionary(
        dictionary: &Dictionary,
        objects: &ObjectCollection,
    ) -> Result<Self::ResultType, Self::ErrorType> {
        let default_width = dictionary.get_number("DW").unwrap_or(Self::DEFAULT_WIDTH);

        let widths_map = dictionary
            .get_array("W")
            .map(|array| GlyphWidthsMap::from_array(array))
            .transpose()?;

        let subtype = dictionary
            .get_string("Subtype")
            .ok_or(CidFontError::MissingSubType)?
            .to_string();

        // According to PDF 1.7 Spec (Table 114), FontDescriptor must be an indirect reference.
        let descriptor = match dictionary.get("FontDescriptor") {
            Some(obj_var_box) => match obj_var_box.as_ref() {
                ObjectVariant::Reference(obj_num) => {
                    let desc_dict = objects
                        .get_dictionary(*obj_num)
                        .ok_or_else(|| CidFontError::InvalidFontDescriptorReference(*obj_num))?;
                    FontDescriptor::from_dictionary(desc_dict.as_ref(), objects)
                        .map_err(CidFontError::FontDescriptorError)?
                }
                other => {
                    // FontDescriptor is present, but not an indirect reference as required.
                    return Err(CidFontError::InvalidEntryType {
                        entry_name: "FontDescriptor",
                        expected_type: "Indirect Reference",
                        found_type: other.name(),
                    });
                }
            },
            None => return Err(CidFontError::MissingFontDescriptor),
        };

        Ok(Self {
            default_width,
            descriptor,
            subtype,
            widths: widths_map,
        })
    }
}
