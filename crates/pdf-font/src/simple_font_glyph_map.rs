use std::collections::HashMap;

use pdf_object::{
    dictionary::Dictionary, error::ObjectError, object_collection::ObjectCollection,
    traits::FromDictionary,
};
use thiserror::Error;

/// Errors that can occur during SimpleFontGlyphWidthsMap parsing.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum SimpleFontGlyphWidthsMapError {
    #[error("Invalid /Widths array length")]
    InvalidWidthArrayLength,
    #[error("{0}")]
    ObjectError(#[from] ObjectError),
}

/// Represents a simple font's glyph widths map parsed from a
/// `/Type1`, `/TrueType`, or `/Type3` font.
pub struct SimpleFontGlyphWidthsMap {
    /// Widths for character codes 0..=255 if provided via /Widths.
    /// Index is the character code, value is the width.
    pub widths: Option<HashMap<u16, f32>>,
    /// First character code in widths array (/FirstChar).
    pub first_char: u16,
    /// Last character code in widths array (/LastChar).
    pub last_char: u16,
}

impl FromDictionary for SimpleFontGlyphWidthsMap {
    const KEY: &'static str = "Widths";

    type ResultType = Self;

    type ErrorType = SimpleFontGlyphWidthsMapError;

    fn from_dictionary(
        dictionary: &Dictionary,
        _objects: &ObjectCollection,
    ) -> Result<Self::ResultType, Self::ErrorType> {
        // Read required fields /FirstChar and /LastChar fields.
        let first_char = dictionary.get_or_err("FirstChar")?.as_number::<u16>()?;
        let last_char = dictionary.get_or_err("LastChar")?.as_number::<u16>()?;

        let Some(widths_obj) = dictionary.get(Self::KEY) else {
            return Ok(SimpleFontGlyphWidthsMap {
                widths: None,
                first_char,
                last_char,
            });
        };

        let arr = widths_obj.try_array()?;

        // Map sequentially: widths[i] corresponds to code (fc + i)
        let mut widths = HashMap::new();
        for (i, w) in arr.iter().enumerate() {
            let Some(i_u16) = u16::try_from(i).ok() else {
                break;
            };
            let code = first_char.saturating_add(i_u16);
            if code > last_char {
                break;
            }
            let width = w.as_number::<f32>()?;
            widths.insert(code, width);
        }

        Ok(SimpleFontGlyphWidthsMap {
            widths: Some(widths),
            first_char,
            last_char,
        })
    }
}

impl SimpleFontGlyphWidthsMap {
    /// Get the width for a given character code, if available.
    /// Returns None if widths are not defined or the code is out of range.
    pub fn get_width(&self, char_code: u16) -> Option<f32> {
        if let Some(ref widths_map) = self.widths {
            widths_map.get(&char_code).copied()
        } else {
            None
        }
    }
}
