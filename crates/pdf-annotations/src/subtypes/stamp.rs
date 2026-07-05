use pdf_object::{dictionary::Dictionary, object_resolver::ObjectResolver};

use crate::AnnotationError;

/// Annotation-specific stamp state.
#[derive(Debug, Clone, PartialEq)]
pub struct StampAnnotation {
    /// The stamp name.
    pub name: Option<Vec<u8>>,
}

impl StampAnnotation {
    pub(crate) fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Self, AnnotationError> {
        let name = dictionary
            .get("Name")
            .map(|value| value.try_bytes_vec(objects))
            .transpose()?;

        Ok(Self { name })
    }
}
