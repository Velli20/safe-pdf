use std::collections::HashMap;

use pdf_object::{
    dictionary::Dictionary, error::ObjectError, object_collection::ObjectCollection,
    traits::FromDictionary,
};
use thiserror::Error;

use crate::font_descriptor::{FontDescriptor, FontDescriptorError};

/// Minimal, initial representation of a PDF TrueType (simple) font.
///
/// Similar to Type1 parsing logic we capture dictionary level metadata
/// required for basic width metrics and embedded program access. Complex
/// encoding differences, glyph substitution, etc. are deferred.
#[derive(Debug)]
pub struct TrueTypeFont {
    /// PostScript base font name (e.g., /ArialMT)
    pub base_font: String,
    /// Optional font descriptor referencing embedded TrueType program (/FontFile2)
    pub font_file: Option<pdf_object::stream::StreamObject>,
    /// Widths for character codes 0..=255 if provided via /Widths.
    pub widths: Option<HashMap<u8, f32>>,
    /// First character code in widths array (/FirstChar)
    pub first_char: Option<u8>,
    /// Last character code in widths array (/LastChar)
    pub last_char: Option<u8>,
}

#[derive(Debug, Error, PartialEq)]
pub enum TrueTypeFontError {
    #[error("{0}")]
    ObjectError(#[from] ObjectError),
    #[error("FontDescriptor error: {0}")]
    FontDescriptor(#[from] FontDescriptorError),
}

impl FromDictionary for TrueTypeFont {
    const KEY: &'static str = "Font";
    type ResultType = Self;
    type ErrorType = TrueTypeFontError;

    fn from_dictionary(
        dictionary: &Dictionary,
        objects: &ObjectCollection,
    ) -> Result<Self::ResultType, Self::ErrorType> {
        let base_font = dictionary
            .get("BaseFont")
            .and_then(|v| v.as_str().map(|s| s.into_owned()))
            .unwrap_or_default();

        // Descriptor is optional for the 14 standard fonts; attempt to resolve if present.
        let font_file = if let Some(fd_obj) = dictionary.get("FontDescriptor") {
            let fd_dict = objects.resolve_dictionary(fd_obj)?;
            let FontDescriptor { font_file } =
                FontDescriptor::from_dictionary(fd_dict, objects)?;
            font_file
        } else {
            None
        };

        // Width extraction mirrors Type1 implementation.
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
                let mut map = HashMap::new();
                for (i, w) in arr.iter().enumerate() {
                    let Some(i_u16) = u16::try_from(i).ok() else { break; };
                    let code_u16 = u16::from(fc).saturating_add(i_u16);
                    let Ok(code) = u8::try_from(code_u16) else { break; };
                    if code > lc { break; }
                    let width = w.as_number::<f32>()?;
                    map.insert(code, width);
                }
                Some(map)
            } else { None }
        } else { None };

        Ok(Self { base_font, font_file, widths, first_char, last_char })
    }
}
