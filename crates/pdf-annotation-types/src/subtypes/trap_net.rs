use pdf_object_reader::{dictionary::Dictionary, object_resolver::ObjectResolver};

use crate::AnnotationError;

/// Trap network annotations carry trap information only.
pub struct TrapNetAnnotation;

impl TrapNetAnnotation {
    pub(crate) fn from_dictionary(
        _dictionary: &Dictionary,
        _objects: &dyn ObjectResolver,
    ) -> Result<Self, AnnotationError> {
        Ok(Self)
    }
}
