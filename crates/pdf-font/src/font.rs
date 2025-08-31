use std::borrow::Cow;

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

impl From<Cow<'_, str>> for FontEncoding {
    fn from(s: Cow<'_, str>) -> Self {
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
    #[error("Invalid /DescendantFonts entry in Type0 font: {0}")]
    InvalidDescendantFonts(&'static str),
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
    #[error("Unsupported or invalid font subtype '{0}'")]
    InvalidFontSubtype(String),
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum FontSubType {
    Type0,
    Type1,
    Type3,
}

impl From<Cow<'_, str>> for FontSubType {
    fn from(s: Cow<'_, str>) -> Self {
        match s.as_ref() {
            "Type0" => FontSubType::Type0,
            "Type1" => FontSubType::Type1,
            "Type3" => FontSubType::Type3,
            _ => FontSubType::Type1,
        }
    }
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
        let base_font = dictionary
            .get("BaseFont")
            .and_then(|v| v.as_str().map(|s| s.into_owned()))
            .unwrap_or_default();

        // Attempt to extract the `/Encoding` entry, if present, and convert it to a `FontEncoding`.
        let encoding = dictionary
            .get("Encoding")
            .and_then(|v| v.as_str())
            .map(FontEncoding::from);

        // Attempt to resolve the optional `/ToUnicode` CMap stream, which maps character codes to Unicode.
        // If present, parse it into a `CharacterMap`. If not present, set cmap to None.
        let cmap = dictionary
            .get("ToUnicode")
            .map(|obj| objects.resolve_stream(obj))
            .transpose()?
            .map(CharacterMap::from_stream_object)
            .transpose()?;

        // Determine the font subtype from the dictionary.
        let subtype = dictionary
            .get_or_err("Subtype")?
            .try_str()
            .map(FontSubType::from)?;

        // If the font is a Type3 font, delegate parsing to the Type3Font handler and return early.
        if subtype == FontSubType::Type3 {
            let type3_font = Type3Font::from_dictionary(dictionary, objects)?;
            return Ok(Self {
                base_font,
                subtype,
                cmap,
                cid_font: None,
                type3_font: Some(type3_font),
                encoding,
            });
        }

        // Only Type0 fonts are supported beyond this point. If not Type0, return an error.
        if subtype != FontSubType::Type0 {
            return Err(FontError::UnsupportedFontSubtype {
                subtype,
                font_type: "Type0",
            });
        }

        // The `/DescendantFonts` array is required for Type0 fonts. Return an error if missing.
        let descendant_fonts_array = dictionary.get_or_err("DescendantFonts")?.try_array()?;

        // The array must not be empty. Get the first element, which should reference the CIDFont dictionary.
        let cid_font_ref_val = descendant_fonts_array
            .first()
            .ok_or(FontError::InvalidDescendantFonts("Array is empty"))?;

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
