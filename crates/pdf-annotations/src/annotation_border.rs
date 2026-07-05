use pdf_object::{dictionary::Dictionary, object_resolver::ObjectResolver};

use crate::AnnotationError;

/// A border array value.
#[derive(Debug, Clone, PartialEq)]
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
        let Some(value) = dictionary.get("Border") else {
            return Ok(None);
        };

        let [horizontal_radius, vertical_radius, width, remaining @ ..] = value
            .try_array_of::<f32, 3>(objects)
            .map_err(|_| AnnotationError::InvalidEntry {
                entry: "Border",
                reason: "expected an array with at least 3 numbers".to_owned(),
            })?;

        let dash_pattern = if remaining.is_empty() {
            None
        } else {
            Some(remaining.to_vec())
        };

        Ok(Some(Self {
            horizontal_radius,
            vertical_radius,
            width,
            dash_pattern,
        }))
    }
}
