use std::collections::HashMap;

use pdf_object::{dictionary::Dictionary, object_resolver::ObjectResolver, traits::FromDictionary};

use crate::font::FontError;

pub struct SimpleFontGlyphWidthsMap;

impl FromDictionary for SimpleFontGlyphWidthsMap {
    const KEY: &'static str = "Widths";
    type ResultType = Option<HashMap<u16, f32>>;
    type ErrorType = FontError;

    fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Self::ResultType, Self::ErrorType> {
        // Read required fields /FirstChar entry.
        let first_char = dictionary
            .get_or_err("FirstChar")?
            .try_number::<u16>(objects)?;

        let Some(widths_obj) = dictionary.get(Self::KEY) else {
            return Ok(None);
        };

        let arr = widths_obj.try_array(objects)?;

        // Map sequentially: widths[i] corresponds to code (fc + i)
        let mut widths = HashMap::new();
        for (i, w) in arr.iter().enumerate() {
            let Some(i_u16) = u16::try_from(i).ok() else {
                break;
            };
            let code = first_char.saturating_add(i_u16);
            let width = w.try_number::<f32>(objects)?;
            widths.insert(code, width);
        }

        Ok(Some(widths))
    }
}
