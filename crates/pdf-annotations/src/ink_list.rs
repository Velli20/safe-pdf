use pdf_object::{dictionary::Dictionary, object_resolver::ObjectResolver};

use crate::{AnnotationError, helpers};

/// A parsed ink list.
#[derive(Debug, Clone, PartialEq)]
pub struct InkList {
    /// The parsed stroke lists.
    pub strokes: Vec<Vec<[f32; 2]>>,
}

impl InkList {
    pub(crate) fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Option<Self>, AnnotationError> {
        let Some(value) = dictionary.get("InkList") else {
            return Ok(None);
        };

        let strokes = value.try_array(objects)?;
        let mut parsed = Vec::with_capacity(strokes.len());

        for stroke in strokes {
            parsed.push(helpers::point_list("InkList", stroke, objects)?);
        }

        Ok(Some(Self { strokes: parsed }))
    }
}
