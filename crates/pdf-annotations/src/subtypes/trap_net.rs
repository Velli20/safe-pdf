use pdf_object::{dictionary::Dictionary, object_resolver::ObjectResolver};

use crate::AnnotationError;

/// Trap network annotations carry trap information only.
#[derive(Debug, Clone, PartialEq)]
pub struct TrapNetAnnotation;

impl TrapNetAnnotation {
    pub(crate) fn from_dictionary(
        _dictionary: &Dictionary,
        _objects: &dyn ObjectResolver,
    ) -> Result<Self, AnnotationError> {
        Ok(Self)
    }
}
