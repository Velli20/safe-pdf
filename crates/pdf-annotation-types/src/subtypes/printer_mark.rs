use pdf_object::{dictionary::Dictionary, object_resolver::ObjectResolver};

use crate::AnnotationError;

/// Printer mark annotations carry printer-specific metadata only.
pub struct PrinterMarkAnnotation;

impl PrinterMarkAnnotation {
    pub(crate) fn from_dictionary(
        _dictionary: &Dictionary,
        _objects: &dyn ObjectResolver,
    ) -> Result<Self, AnnotationError> {
        Ok(Self)
    }
}
