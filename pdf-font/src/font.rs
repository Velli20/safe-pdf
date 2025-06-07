use pdf_object::{
    ObjectVariant, Value,
    dictionary::Dictionary,
    object_collection::ObjectCollection,
    traits::{FromDictionary, FromStreamObject},
};

use crate::{characther_map::CharacterMap, cid_font::CharacterIdentifierFont, error::FontError};

/// Represents a Type0 font, a composite font type in PDF.
///
/// Type0 fonts are used to organize fonts that have a large number of characters,
/// such as those for East Asian languages (Chinese, Japanese, Korean).
pub struct Font {
    /// The PostScript name of the font. For Type0 fonts, this is the
    /// name of the Type0 font itself, not the CIDFont.
    base_font: String,
    /// The font subtype. For Type0 fonts, this value must be `/Type0`.
    subtype: String,
    /// A stream defining a CMap that maps character codes to Unicode values.
    pub cmap: Option<CharacterMap>,
    /// (Required for Type0 fonts) The CIDFont dictionary that is the descendant of this Type0 font.
    /// This CIDFont provides the actual glyph descriptions.
    pub cid_font: CharacterIdentifierFont,
}

impl FromDictionary for Font {
    const KEY: &'static str = "Font";
    type ResultType = Self;
    type ErrorType = FontError;

    fn from_dictionary(
        dictionary: &Dictionary,
        objects: &ObjectCollection,
    ) -> Result<Self::ResultType, Self::ErrorType> {
        let base_font = dictionary.get_string("BaseFont").unwrap().clone();
        let subtype = dictionary.get_string("Subtype").unwrap().clone();

        let cmap: Option<CharacterMap> = dictionary
            .get_object("ToUnicode")
            .map(|to_unicode_obj| match to_unicode_obj {
                ObjectVariant::Reference(num) => objects
                    .get(*num)
                    .ok_or(FontError::MissingFontDescriptor)
                    .and_then(|resolved_obj| match resolved_obj {
                        ObjectVariant::Stream(s) => CharacterMap::from_stream_object(&s),
                        _ => Err(FontError::MissingFontDescriptor),
                    }),
                _ => Err(FontError::MissingFontDescriptor),
            })
            .transpose()?;

        // This should be a single element array if the SubType is Type0.
        let descentant_fonts = dictionary
            .get_array("DescendantFonts")
            .ok_or(FontError::MissingDescendantFonts)?;

        let cid_font = if let Some(Value::IndirectObject(ObjectVariant::Reference(num))) =
            descentant_fonts.0.first()
        {
            if let Some(s) = objects.get_dictionary(*num) {
                CharacterIdentifierFont::from_dictionary(s, objects)?
            } else {
                return Err(FontError::MissingCharacterIdentifierFont);
            }
        } else {
            return Err(FontError::MissingCharacterIdentifierFont);
        };
        return Ok(Self {
            base_font,
            subtype,
            cmap,
            cid_font,
        });
    }
}
