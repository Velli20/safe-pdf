use pdf_object::{
    ObjectVariant, Value,
    dictionary::Dictionary,
    object_collection::ObjectCollection,
    traits::{FromDictionary, FromStreamObject},
};

use crate::{characther_map::CharacterMap, cid_font::CharacterIdentifierFont, error::FontError};

pub enum FontEncoding {
    /// No remapping, character codes are interpreted directly as CIDs in vertical writing mode.
    IdentityVertical,
    /// No remapping, character codes are interpreted directly as CIDs in horizontal writing mode.
    IdentityHorizontal,
    /// Unknown encoding.
    Unknown(String),
}

impl From<&String> for FontEncoding {
    fn from(s: &String) -> Self {
        if s == "Identity-H" {
            FontEncoding::IdentityHorizontal
        } else if s == "Identity-V" {
            FontEncoding::IdentityVertical
        } else {
            FontEncoding::Unknown(s.to_string())
        }
    }
}

/// Represents a Type0 font, a composite font type in PDF.
///
/// Type0 fonts are used to organize fonts that have a large number of characters,
/// such as those for East Asian languages (Chinese, Japanese, Korean).
pub struct Font {
    /// The PostScript name of the font. For Type0 fonts, this is the
    /// name of the Type0 font itself, not the CIDFont.
    base_font: String,
    /// The font subtype. For Type0 fonts, this value must be `/Type0`.
    pub subtype: String,
    /// A stream defining a CMap that maps character codes to Unicode values.
    pub cmap: Option<CharacterMap>,
    /// (Required for Type0 fonts) The CIDFont dictionary that is the descendant of this Type0 font.
    /// This CIDFont provides the actual glyph descriptions.
    pub cid_font: CharacterIdentifierFont,
    pub encoding: Option<FontEncoding>,
}

impl FromDictionary for Font {
    const KEY: &'static str = "Font";
    type ResultType = Self;
    type ErrorType = FontError;

    fn from_dictionary(
        dictionary: &Dictionary,
        objects: &ObjectCollection,
    ) -> Result<Self::ResultType, Self::ErrorType> {
        let dict_type = "Type0 Font";

        let base_font = dictionary
            .get_string("BaseFont")
            .ok_or(FontError::MissingEntry {
                entry_name: "BaseFont",
                dictionary_type: dict_type,
            })?
            .clone();

        let subtype = dictionary
            .get_string("Subtype")
            .ok_or(FontError::MissingEntry {
                entry_name: "Subtype",
                dictionary_type: dict_type,
            })?
            .clone();

        if subtype != "Type0" {
            return Err(FontError::UnsupportedFontSubtype {
                subtype,
                font_type: "Type0",
            });
        }

        let cmap: Option<CharacterMap> = dictionary
            .get_object("ToUnicode")
            .map(|to_unicode_obj_variant| match to_unicode_obj_variant {
                ObjectVariant::Reference(num) => objects
                    .get(*num)
                    .ok_or_else(|| {
                        FontError::ToUnicodeResolution(format!(
                            "Could not resolve /ToUnicode reference: {}",
                            num
                        ))
                    })
                    .and_then(|resolved_obj| match resolved_obj {
                        ObjectVariant::Stream(s) => CharacterMap::from_stream_object(&s),
                        other => Err(FontError::InvalidEntryType {
                            entry_name: "/ToUnicode (resolved)",
                            dictionary_type: dict_type,
                            expected_type: "Stream",
                            found_type: other.name().to_string(),
                        }),
                    }),
                ObjectVariant::Stream(s) => CharacterMap::from_stream_object(s),
                other => Err(FontError::InvalidEntryType {
                    entry_name: "/ToUnicode",
                    dictionary_type: dict_type,
                    expected_type: "Reference or Stream",
                    found_type: other.name().to_string(),
                }),
            })
            .transpose()?;

        let encoding = dictionary.get_string("Encoding").map(FontEncoding::from);

        let descendant_fonts_array = dictionary
            .get_array("DescendantFonts")
            .ok_or(FontError::MissingDescendantFonts)?;

        let cid_font_ref_val = descendant_fonts_array
            .0
            .first()
            .ok_or_else(|| FontError::InvalidDescendantFonts("Array is empty".to_string()))?;

        let cid_font = match cid_font_ref_val {
            Value::IndirectObject(ObjectVariant::Reference(num)) => objects
                .get_dictionary(*num)
                .ok_or_else(|| {
                    FontError::InvalidDescendantFonts(format!(
                        "Could not resolve CIDFont reference {} from /DescendantFonts",
                        num
                    ))
                })
                .and_then(|cid_font_dict| {
                    CharacterIdentifierFont::from_dictionary(cid_font_dict, objects)
                        .map_err(|e| FontError::DescendantCIDFontError { cause: Box::new(e) })
                })?,
            other => {
                return Err(FontError::InvalidEntryType {
                    entry_name: "/DescendantFonts[0]",
                    dictionary_type: dict_type,
                    expected_type: "IndirectObject (Reference to Dictionary)",
                    found_type: other.name().to_string(),
                });
            }
        };
        Ok(Self {
            base_font,
            subtype,
            cmap,
            cid_font,
            encoding,
        })
    }
}
