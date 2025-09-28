use std::collections::HashMap;

use pdf_object::{
    dictionary::Dictionary, error::ObjectError, object_collection::ObjectCollection,
    stream::StreamObject, traits::FromDictionary,
};
use thiserror::Error;

use crate::font_descriptor::{FontDescriptor, FontDescriptorError};

/// Minimal, initial representation of a PDF Type1 font.
///
/// This focuses on dictionary-level metadata needed by higher layers
/// and defers actual glyph rendering or embedded program parsing.
#[derive(Debug)]
pub struct Type1Font {
    /// PostScript base font name (e.g., /Helvetica)
    pub base_font: String,
    /// A stream containing the font program.
    pub font_file: StreamObject,
    /// Optional encoding name (e.g., /WinAnsiEncoding) or custom encoding via Differences
    /// For now we capture only the base encoding name for quick wiring; differences can be
    /// expanded later similarly to Type3.
    pub base_encoding: Option<String>,
    /// Widths for character codes 0..=255 if provided via /Widths.
    /// Index is the character code, value is the width.
    pub widths: Option<HashMap<u8, f32>>, // simple map for now; may be replaced by a compact Vec
    /// First character code in widths array
    pub first_char: Option<u8>,
    /// Last character code in widths array
    pub last_char: Option<u8>,
}

/// Errors that can occur while parsing a Type1 font dictionary.
#[derive(Debug, Error, PartialEq)]
pub enum Type1FontError {
    #[error("{0}")]
    ObjectError(#[from] ObjectError),
    #[error("FontDescriptor error: {0}")]
    FontDescriptor(#[from] FontDescriptorError),
}

impl FromDictionary for Type1Font {
    const KEY: &'static str = "Font";
    type ResultType = Self;
    type ErrorType = Type1FontError;

    fn from_dictionary(
        dictionary: &Dictionary,
        objects: &ObjectCollection,
    ) -> Result<Self::ResultType, Self::ErrorType> {
        // BaseFont is recommended for Type1. Default to empty string if missing.
        let base_font = dictionary
            .get("BaseFont")
            .and_then(|v| v.as_str().map(|s| s.into_owned()))
            .unwrap_or_default();

        // Read '/FontDescriptor’.
        let fd = dictionary.get_or_err("FontDescriptor")?;
        let FontDescriptor { font_file } =
            FontDescriptor::from_dictionary(objects.resolve_dictionary(fd)?, objects)?;

        // Encoding may be a name or a dictionary. For initial support, record only base name.
        let base_encoding = dictionary
            .get("Encoding")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // Parse optional widths: /FirstChar, /LastChar, /Widths [ ... ]
        let first_char = dictionary
            .get("FirstChar")
            .map(pdf_object::ObjectVariant::as_number::<i64>)
            .transpose()?
            .and_then(|i| u8::try_from(i).ok());

        let last_char = dictionary
            .get("LastChar")
            .map(pdf_object::ObjectVariant::as_number::<i64>)
            .transpose()?
            .and_then(|i| u8::try_from(i).ok());

        let widths = if let (Some(fc), Some(lc)) = (first_char, last_char) {
            if let Some(widths_obj) = dictionary.get("Widths") {
                let arr = widths_obj.try_array()?;
                // Map sequentially: widths[i] corresponds to code (fc + i)
                let mut map = HashMap::new();
                for (i, w) in arr.iter().enumerate() {
                    let Some(i_u16) = u16::try_from(i).ok() else {
                        break;
                    };
                    let code_u16 = u16::from(fc).saturating_add(i_u16);
                    let Ok(code) = u8::try_from(code_u16) else {
                        break;
                    };
                    if code > lc {
                        break;
                    }
                    let width = w.as_number::<f32>()?;
                    map.insert(code, width);
                }
                Some(map)
            } else {
                None
            }
        } else {
            None
        };

        Ok(Self {
            base_font,
            font_file,
            base_encoding,
            widths,
            first_char,
            last_char,
        })
    }
}
