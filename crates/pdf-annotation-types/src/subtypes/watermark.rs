use pdf_object_reader::{dictionary::Dictionary, object_resolver::ObjectResolver};

use crate::AnnotationError;

/// Watermark annotations carry watermark metadata only.
pub struct WatermarkAnnotation;

impl WatermarkAnnotation {
    pub(crate) fn from_dictionary(
        _dictionary: &Dictionary,
        _objects: &dyn ObjectResolver,
    ) -> Result<Self, AnnotationError> {
        Ok(Self)
    }
}
