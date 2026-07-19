use pdf_object::{
    dictionary::Dictionary, object_lookup::ObjectLookupExt, object_resolver::ObjectResolver,
};

use crate::AnnotationError;

/// A border array value.
#[derive(Clone, Debug, PartialEq)]
pub struct AnnotationBorder {
    /// Horizontal corner radius.
    pub horizontal_radius: f32,
    /// Vertical corner radius.
    pub vertical_radius: f32,
    /// Border width.
    pub width: f32,
    /// Optional dash pattern.
    pub dash_pattern: Option<Vec<f32>>,
}

impl AnnotationBorder {
    pub(crate) fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Option<Self>, AnnotationError> {
        let Some(value) = dictionary.optional_array("Border", objects)? else {
            return Ok(None);
        };

        let [horizontal_radius, vertical_radius, width, rest @ ..] = value else {
            return Err(AnnotationError::InvalidEntry {
                entry: "Border",
                reason: "expected an array with at least 3 numbers".to_owned(),
            });
        };

        let horizontal_radius = horizontal_radius.try_number::<f32>(objects)?;
        let vertical_radius = vertical_radius.try_number::<f32>(objects)?;
        let width = width.try_number::<f32>(objects)?;

        let mut values = rest.iter();

        let dash_pattern = match values.next() {
            Some(first_dash) => match first_dash.try_array(objects) {
                Ok(dash_array) => Some(
                    dash_array
                        .iter()
                        .map(|value| value.try_number::<f32>(objects))
                        .collect::<Result<Vec<_>, _>>()?,
                ),
                Err(_) => {
                    let mut pattern = vec![first_dash.try_number::<f32>(objects)?];
                    for value in values {
                        pattern.push(value.try_number::<f32>(objects)?);
                    }
                    Some(pattern)
                }
            },
            None => None,
        };

        Ok(Some(Self {
            horizontal_radius,
            vertical_radius,
            width,
            dash_pattern,
        }))
    }
}
