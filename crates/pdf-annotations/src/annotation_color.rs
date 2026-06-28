use pdf_object::{dictionary::Dictionary, object_resolver::ObjectResolver};

use crate::AnnotationError;

/// A parsed annotation color array.
#[derive(Debug, Clone, PartialEq)]
pub struct AnnotationColor {
    /// Raw numeric color components.
    pub components: Vec<f32>,
}

impl AnnotationColor {
    pub(crate) fn from_dictionary(
        dictionary: &Dictionary,
        key: &'static str,
        objects: &dyn ObjectResolver,
    ) -> Result<Option<Self>, AnnotationError> {
        dictionary
            .get(key)
            .map(|value| {
                Ok(Self {
                    components: value.try_vec_of::<f32>(objects)?,
                })
            })
            .transpose()
    }
}
