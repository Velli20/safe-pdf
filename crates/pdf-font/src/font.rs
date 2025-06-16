use pdf_object::{
    ObjectVariant,
    dictionary::Dictionary,
    object_collection::ObjectCollection,
    traits::{FromDictionary, FromStreamObject},
};
use thiserror::Error;

use crate::{
    characther_map::{CMapError, CharacterMap},
    cid_font::{CharacterIdentifierFont, CidFontError},
};

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

/// Defines errors that can occur while reading a font object.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum FontError {
    /// A required dictionary entry was missing.
    #[error("Missing required entry '{entry_name}' in {dictionary_type} dictionary")]
    MissingEntry {
        entry_name: &'static str,
        dictionary_type: &'static str,
    },

    /// A dictionary entry had an unexpected type.
    #[error(
        "Entry '{entry_name}' in Type0 Font dictionary has invalid type: expected {expected_type}, found {found_type}"
    )]
    InvalidEntryType {
        entry_name: &'static str,
        expected_type: &'static str,
        found_type: &'static str,
    },

    /// Indicates that the required `/DescendantFonts` entry is missing in a Type0 font dictionary.
    #[error("Missing /DescendantFonts entry in Type0 font")]
    MissingDescendantFonts,
    /// The `/DescendantFonts` array in a Type0 font is empty or invalid.
    #[error("Invalid /DescendantFonts entry in Type0 font: {0}")]
    InvalidDescendantFonts(String),
    /// Failed to resolve or parse the descendant CIDFont from a Type0 font.
    #[error("Error processing descendant CIDFont for Type0 font")]
    DescendantCIDFontError(#[from] CidFontError),
    /// Error related to the `/ToUnicode` CMap processing (e.g., reference resolution).
    #[error("Error processing /ToUnicode CMap: {0}")]
    ToUnicodeResolution(String),
    /// Indicates an error occurred while parsing a Character Map (CMap) stream.
    #[error("CMap parsing error: {0}")]
    CMapParse(#[from] CMapError),
    /// The font subtype is unsupported or invalid for the current parsing context.
    #[error("Unsupported or invalid font subtype '{subtype}' for {font_type} font")]
    UnsupportedFontSubtype {
        subtype: String,
        font_type: &'static str,
    },
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
        let base_font = dictionary
            .get_string("BaseFont")
            .ok_or(FontError::MissingEntry {
                entry_name: "BaseFont",
                dictionary_type: "Type0 Font",
            })?
            .clone();

        let subtype = dictionary
            .get_string("Subtype")
            .ok_or(FontError::MissingEntry {
                entry_name: "Subtype",
                dictionary_type: "Type0 Font",
            })?
            .clone();

        if subtype != "Type0" {
            return Err(FontError::UnsupportedFontSubtype {
                subtype,
                font_type: "Type0",
            });
        }

        let cmap = if let Some(obj) = dictionary.get("ToUnicode") {
            match obj.as_ref() {
                ObjectVariant::Reference(num) => {
                    let resolved_obj = objects.get(*num).ok_or_else(|| {
                        FontError::ToUnicodeResolution(format!(
                            "Could not resolve /ToUnicode reference: {}",
                            num
                        ))
                    })?;

                    match resolved_obj {
                        ObjectVariant::Stream(s) => Some(CharacterMap::from_stream_object(&s)?),
                        other => {
                            return Err(FontError::InvalidEntryType {
                                entry_name: "/ToUnicode (resolved)",
                                expected_type: "Stream",
                                found_type: other.name(),
                            });
                        }
                    }
                }
                ObjectVariant::Stream(s) => Some(CharacterMap::from_stream_object(s)?),
                other => {
                    return Err(FontError::InvalidEntryType {
                        entry_name: "/ToUnicode",
                        expected_type: "Reference or Stream",
                        found_type: other.name(),
                    });
                }
            }
        } else {
            None
        };

        let encoding = dictionary.get_string("Encoding").map(FontEncoding::from);

        let descendant_fonts_array = dictionary
            .get_array("DescendantFonts")
            .ok_or(FontError::MissingDescendantFonts)?;

        let cid_font_ref_val = descendant_fonts_array
            .first()
            .ok_or_else(|| FontError::InvalidDescendantFonts("Array is empty".to_string()))?;

        let cid_font = match cid_font_ref_val {
            ObjectVariant::Reference(num) => {
                let dictionary = objects.get_dictionary(*num).ok_or_else(|| {
                    FontError::InvalidDescendantFonts(format!(
                        "Could not resolve CIDFont reference {} from /DescendantFonts",
                        num
                    ))
                })?;

                CharacterIdentifierFont::from_dictionary(dictionary, objects)?
            }
            other => {
                return Err(FontError::InvalidEntryType {
                    entry_name: "/DescendantFonts[0]",
                    expected_type: "IndirectObject (Reference to Dictionary)",
                    found_type: other.name(),
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
