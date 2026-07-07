use pdf_object::{
    dictionary::Dictionary, object_lookup::ObjectLookupExt, object_resolver::ObjectResolver,
};

use crate::{AnnotationError, BorderStyleName};

/// A border style dictionary.
pub struct BorderStyle {
    /// Border width.
    pub width: Option<f32>,
    /// Border style name.
    pub style: Option<BorderStyleName>,
    /// Optional dash pattern.
    pub dash_pattern: Option<Vec<f32>>,
}

impl BorderStyle {
    pub(crate) fn from_dictionary(
        dictionary: &Dictionary,
        key: &'static str,
        objects: &dyn ObjectResolver,
    ) -> Result<Option<Self>, AnnotationError> {
        let Some(value) = dictionary.get(key) else {
            return Ok(None);
        };

        let dictionary = value.try_dictionary(objects)?;
        let width = dictionary.optional_number::<f32>("W", objects)?;
        let style = dictionary
            .get("S")
            .map(|value| {
                value
                    .try_str(objects)
                    .map(|name| BorderStyleName::from(name.as_ref()))
            })
            .transpose()?;
        let dash_pattern = dictionary.optional_vec_of::<f32>("D", objects)?;

        Ok(Some(Self {
            width,
            style,
            dash_pattern,
        }))
    }
}
