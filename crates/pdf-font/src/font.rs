use std::borrow::Cow;

use pdf_object::{
    dictionary::Dictionary, error::ObjectError, object_collection::ObjectCollection,
    traits::FromDictionary,
};
use thiserror::Error;

use crate::{
    character_map::CMapError,
    true_type_font::{TrueTypeFont, TrueTypeFontError},
    type0_font::{Type0Font, Type0FontError},
    type1_font::{Type1Font, Type1FontError},
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
    #[error("Error processing descendant CIDFont for Type0 font")]
    DescendantCIDFontError(#[from] Type0FontError),
    #[error("CMap parsing error: {0}")]
    CMapParse(#[from] CMapError),
    #[error("{0}")]
    ObjectError(#[from] ObjectError),
    #[error("Error processing Type3 font: {0}")]
    Type3FontError(#[from] Type3FontError),
    #[error("Error processing Type1 font: {0}")]
    Type1FontError(#[from] Type1FontError),
    #[error("Error processing TrueType font: {0}")]
    TrueTypeFontError(#[from] TrueTypeFontError),
    #[error("Unsupported or invalid font subtype '{subtype}'")]
    UnsupportedFontSubtype { subtype: String },
    #[error("Unsupported or invalid font subtype '{0}'")]
    InvalidFontSubtype(String),
}

/// Represents a font object in a PDF document.
pub enum Font {
    /// A CIDFont used as a descendant font in a Type0 font.
    Type0(Type0Font),
    /// A classic PostScript font.
    Type1(Type1Font),
    /// A type 3 font with glyphs defined by PDF content streams.
    Type3(Type3Font),
    /// A TrueType font.
    TrueType(TrueTypeFont),
}

impl FromDictionary for Font {
    const KEY: &'static str = "Font";
    type ResultType = Self;
    type ErrorType = FontError;

    fn from_dictionary(
        dictionary: &Dictionary,
        objects: &ObjectCollection,
    ) -> Result<Self::ResultType, Self::ErrorType> {
        // Determine the font subtype from the dictionary.
        let subtype = dictionary.get_or_err("Subtype")?.try_str()?;

        match subtype.as_ref() {
            "Type0" => {
                let type0_font = Type0Font::from_dictionary(dictionary, objects)
                    .map_err(FontError::DescendantCIDFontError)?;
                Ok(Font::Type0(type0_font))
            }
            "Type1" => {
                let type1_font = Type1Font::from_dictionary(dictionary, objects)
                    .map_err(FontError::Type1FontError)?;
                Ok(Font::Type1(type1_font))
            }
            "Type3" => {
                let type3_font = Type3Font::from_dictionary(dictionary, objects)
                    .map_err(FontError::Type3FontError)?;
                Ok(Font::Type3(type3_font))
            }
            "TrueType" => {
                let tt_font = TrueTypeFont::from_dictionary(dictionary, objects)
                    .map_err(FontError::TrueTypeFontError)?;
                Ok(Font::TrueType(tt_font))
            }

            other => Err(FontError::UnsupportedFontSubtype {
                subtype: other.to_string(),
            }),
        }
    }
}
