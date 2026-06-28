use pdf_object::{dictionary::Dictionary, object_resolver::ObjectResolver};

use crate::{AnnotationError, BorderStyleName};

/// A border style dictionary.
#[derive(Debug, Clone, PartialEq)]
pub struct BorderStyle {
    /// Border width.
    pub width: Option<f32>,
    /// Border style name.
    pub style: Option<BorderStyleName>,
    /// Optional dash pattern.
    pub dash_pattern: Option<Vec<f32>>,
    /// The original border style dictionary.
    pub dictionary: Dictionary,
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

        let dictionary = value.try_dictionary(objects)?.clone();
        let width = dictionary
            .get("W")
            .map(|value| value.try_number::<f32>(objects))
            .transpose()?;
        let style = dictionary
            .get("S")
            .map(|value| {
                value
                    .try_str(objects)
                    .map(|name| BorderStyleName::from(name.as_ref()))
            })
            .transpose()?;
        let dash_pattern = dictionary
            .get("D")
            .map(|value| value.try_vec_of::<f32>(objects))
            .transpose()?;

        Ok(Some(Self {
            width,
            style,
            dash_pattern,
            dictionary,
        }))
    }
}
