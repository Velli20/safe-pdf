use std::collections::HashMap;

use pdf_object::{
    ObjectVariant, dictionary::Dictionary, object_collection::ObjectCollection,
    traits::FromDictionary,
};

use crate::{error::FontError, font_descriptor::FontDescriptor, glyph_widths_map::GlyphWidthsMap};

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

impl FromDictionary for CharacterIdentifierFont {
    const KEY: &'static str = "Font";

    type ResultType = Self;
    type ErrorType = FontError;

    fn from_dictionary(
        dictionary: &Dictionary,
        objects: &ObjectCollection,
    ) -> Result<Self::ResultType, Self::ErrorType> {
        let default_width = dictionary.get_number("DW").unwrap_or(Self::DEFAULT_WIDTH);

        // Initialize a map to store parsed CID widths.
        // The key is the starting CID, and the value is a vector of widths
        // for consecutive CIDs starting from the key.
        let widths_map = if let Some(array) = dictionary.get_array("W") {
            Some(GlyphWidthsMap::from_array(array).unwrap())
        } else {
            None
        };

        let subtype = dictionary.get_string("Subtype").cloned().unwrap();

        let descriptor =
            if let Some(ObjectVariant::Reference(num)) = dictionary.get_object("FontDescriptor") {
                if let Some(s) = objects.get_dictionary(*num) {
                    FontDescriptor::from_dictionary(s, objects)?
                } else {
                    return Err(FontError::MissingFontDescriptor);
                }
            } else {
                return Err(FontError::MissingFontDescriptor);
            };
        Ok(Self {
            default_width,
            descriptor,
            subtype,
            widths: widths_map,
        })
    }
}
