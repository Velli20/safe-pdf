use pdf_object::{dictionary::Dictionary, object_resolver::ObjectResolver};

use crate::AnnotationError;

/// A parsed annotation quad-point list.
pub struct QuadPoints {
    /// The parsed quads.
    pub quads: Vec<[f32; 8]>,
}

impl QuadPoints {
    pub(crate) fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Option<Self>, AnnotationError> {
        let Some(value) = dictionary.get("QuadPoints") else {
            return Ok(None);
        };

        let values = value.try_vec_of::<f32>(objects)?;
        if values.len() % 8 != 0 {
            return Err(AnnotationError::InvalidEntry {
                entry: "QuadPoints",
                reason: format!("expected a multiple of 8 numbers, found {}", values.len()),
            });
        }

        let mut quads = Vec::with_capacity(values.len() / 8);
        for chunk in values.chunks_exact(8) {
            let quad: [f32; 8] = chunk
                .try_into()
                .map_err(|_| AnnotationError::InvalidEntry {
                    entry: "QuadPoints",
                    reason: "failed to convert quad points".to_owned(),
                })?;
            quads.push(quad);
        }

        Ok(Some(Self { quads }))
    }
}
