use pdf_object::{
    dictionary::Dictionary,
    error::ObjectError,
    object_collection::ObjectCollection,
    traits::{FromDictionary, FromStreamObject},
};
use thiserror::Error;

use crate::{
    characther_map::{CMapError, CharacterMap},
    cid_font::{CharacterIdentifierFont, CidFontError},
    type3_font::{Type3Font, Type3FontError},
};

pub enum FontEncoding {
    /// No remapping, character codes are interpreted directly as CIDs in vertical writing mode.
    IdentityVertical,
    /// No remapping, character codes are interpreted directly as CIDs in horizontal writing mode.
    IdentityHorizontal,
    /// Unknown encoding.
    Unknown(String),
}

impl From<&str> for FontEncoding {
    fn from(s: &str) -> Self {
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
#[derive(Debug, Error, PartialEq)]
pub enum FontError {
    #[error("Missing required entry '{entry_name}' in {dictionary_type} dictionary")]
    MissingEntry {
        entry_name: &'static str,
        dictionary_type: &'static str,
    },
    #[error(
        "Entry '{entry_name}' in Type0 Font dictionary has invalid type: expected {expected_type}, found {found_type}"
    )]
    InvalidEntryType {
        entry_name: &'static str,
        expected_type: &'static str,
        found_type: &'static str,
    },
    #[error("Missing /DescendantFonts entry in Type0 font")]
    MissingDescendantFonts,
    #[error("Invalid /DescendantFonts entry in Type0 font: {0}")]
    InvalidDescendantFonts(String),
    #[error("Error processing descendant CIDFont for Type0 font")]
    DescendantCIDFontError(#[from] CidFontError),
    #[error("CMap parsing error: {0}")]
    CMapParse(#[from] CMapError),
    #[error("{0}")]
    ObjectError(#[from] ObjectError),
    #[error("Error processing Type3 font: {0}")]
    Type3FontError(#[from] Type3FontError),
    #[error("Unsupported or invalid font subtype '{subtype}' for {font_type} font")]
    UnsupportedFontSubtype {
        subtype: FontSubType,
        font_type: &'static str,
    },
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum FontSubType {
    Type0,
    Type1,
    Type3,
}

impl std::fmt::Display for FontSubType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FontSubType::Type0 => write!(f, "/Type0"),
            FontSubType::Type1 => write!(f, "/Type1"),
            FontSubType::Type3 => write!(f, "/Type3"),
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
    pub base_font: String,
    /// The font subtype.
    pub subtype: FontSubType,
    /// A stream defining a CMap that maps character codes to Unicode values.
    pub cmap: Option<CharacterMap>,
    /// (Required for Type0 fonts) The CIDFont dictionary that is the descendant of this Type0 font.
    /// This CIDFont provides the actual glyph descriptions.
    pub cid_font: Option<CharacterIdentifierFont>,
    pub type3_font: Option<Type3Font>,
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
        let base_font = dictionary.get_string("BaseFont").unwrap_or("").to_owned();

        // Determine the font subtype from the dictionary.
        let subtype = match dictionary.get_string("Subtype") {
            Some("Type0") => FontSubType::Type0,
            Some("Type1") => FontSubType::Type1,
            Some("Type3") => FontSubType::Type3,
            _ => {
                return Err(FontError::MissingEntry {
                    entry_name: "Subtype",
                    dictionary_type: "Type0 Font",
                });
            }
        };

        // If the font is a Type3 font, delegate parsing to the Type3Font handler and return early.
        if subtype == FontSubType::Type3 {
            let type3_font = Type3Font::from_dictionary(dictionary, objects)?;
            return Ok(Self {
                base_font,
                subtype,
                cmap: None,
                cid_font: None,
                type3_font: Some(type3_font),
                encoding: None,
            });
        }

        // Only Type0 fonts are supported beyond this point. If not Type0, return an error.
        if subtype != FontSubType::Type0 {
            return Err(FontError::UnsupportedFontSubtype {
                subtype,
                font_type: "Type0",
            });
        }

        // Attempt to resolve the optional `/ToUnicode` CMap stream, which maps character codes to Unicode.
        // If present, parse it into a `CharacterMap`. If not present, set cmap to None.
        let cmap = if let Some(obj) = dictionary.get("ToUnicode") {
            let stream = objects.resolve_stream(obj.as_ref())?;
            Some(CharacterMap::from_stream_object(&stream)?)
        } else {
            None
        };

        // Attempt to extract the `/Encoding` entry, if present, and convert it to a `FontEncoding`.
        let encoding = dictionary.get_string("Encoding").map(FontEncoding::from);

        // The `/DescendantFonts` array is required for Type0 fonts. Return an error if missing.
        let descendant_fonts_array = dictionary
            .get_array("DescendantFonts")
            .ok_or(FontError::MissingDescendantFonts)?;

        // The array must not be empty. Get the first element, which should reference the CIDFont dictionary.
        let cid_font_ref_val = descendant_fonts_array
            .first()
            .ok_or_else(|| FontError::InvalidDescendantFonts("Array is empty".to_string()))?;

        // Resolve the CIDFont dictionary from the reference and parse it into a `CharacterIdentifierFont`.
        let cid_font = {
            let dictionary = objects.resolve_dictionary(cid_font_ref_val)?;

            CharacterIdentifierFont::from_dictionary(dictionary, objects)?
        };
        Ok(Self {
            base_font,
            subtype,
            cmap,
            cid_font: Some(cid_font),
            type3_font: None,
            encoding,
        })
    }
}
