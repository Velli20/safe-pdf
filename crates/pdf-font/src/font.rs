use std::borrow::Cow;

use pdf_object::{
    dictionary::Dictionary, error::ObjectError, object_resolver::ObjectResolver,
    traits::FromDictionary,
};
use thiserror::Error;

use crate::{
    font_descriptor::FontDescriptorError,
    true_type_font::{TrueTypeFont, TrueTypeFontError},
    type0_font::{Type0Font, Type0FontError},
    type1_font::{Type1Font, Type1FontError},
    type3_font::{Type3Font, Type3FontError},
};

/// Defines errors that can occur while reading a font object.
#[derive(Debug, Error, PartialEq)]
pub enum FontError {
    #[error("{0}")]
    ObjectError(#[from] ObjectError),
    #[error("Error processing Type3 font: {0}")]
    Type3FontError(#[from] Type3FontError),
    #[error("Error processing Type1 font: {0}")]
    Type1FontError(#[from] Type1FontError),
    #[error("Error processing Type0 font: {0}")]
    Type0FontError(#[from] Type0FontError),
    #[error("Error processing TrueType font: {0}")]
    TrueTypeFontError(#[from] TrueTypeFontError),
    #[error("Unsupported or invalid font subtype '{subtype}'")]
    UnsupportedFontSubtype { subtype: String },
    #[error("Unsupported or invalid font subtype '{0}'")]
    InvalidFontSubtype(String),
    #[error("Failed to build font: {0}")]
    FontBuildError(String),
    #[error("FontDescriptor error: {0}")]
    FontDescriptorError(#[from] FontDescriptorError),
    #[error("Encoding reading error: {0}")]
    EncodingReadError(#[from] crate::encoding::EncodingReadError),
    #[error("SimpleFontGlyphWidthsMap parsing error: {0}")]
    SimpleFontGlyphWidthsMapError(
        #[from] crate::simple_font_glyph_map::SimpleFontGlyphWidthsMapError,
    ),
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
        objects: &dyn ObjectResolver,
    ) -> Result<Self::ResultType, Self::ErrorType> {
        // Determine the font subtype from the dictionary.
        let subtype = dictionary.get_or_err("Subtype")?.try_str(objects)?;

        match subtype.as_ref() {
            "Type0" => {
                let type0_font = Type0Font::from_dictionary(dictionary, objects)?;
                Ok(Font::Type0(type0_font))
            }
            "Type1" => {
                let type1_font = Type1Font::from_dictionary(dictionary, objects)?;
                Ok(Font::Type1(type1_font))
            }
            "Type3" => {
                let type3_font = Type3Font::from_dictionary(dictionary, objects)?;
                Ok(Font::Type3(type3_font))
            }
            "TrueType" => {
                let tt_font = TrueTypeFont::from_dictionary(dictionary, objects)?;
                Ok(Font::TrueType(tt_font))
            }
            other => Err(FontError::UnsupportedFontSubtype {
                subtype: other.to_string(),
            }),
        }
    }
}

impl Font {
    pub fn get_glyph_width(&self, char_code: u16) -> f32 {
        match self {
            Font::Type0(font) => {
                if let Some(w) = &font.widths {
                    return w.get_width(char_code).unwrap_or(font.default_width);
                }
                font.default_width
            }
            Font::TrueType(font) => font.widths.get_width(char_code),
            Font::Type1(font) => font.widths.get_width(char_code),
            _ => 0.0,
        }
    }

    pub fn glyph_name(&self, char_code: u16) -> Option<&str> {
        let index = usize::from(char_code);
        match self {
            Font::Type1(font) => font.encoding.names.get(index).map(Cow::as_ref),
            Font::Type3(font) => font
                .encoding
                .as_ref()
                .and_then(|enc| enc.names.get(index).map(Cow::as_ref)),
            _ => None,
        }
    }
}
