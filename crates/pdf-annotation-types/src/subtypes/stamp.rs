use pdf_object::{
    dictionary::Dictionary, object_lookup::ObjectLookupExt, object_resolver::ObjectResolver,
};

use crate::AnnotationError;

/// Annotation-specific stamp state.
pub struct StampAnnotation {
    /// The stamp name.
    pub name: Option<Vec<u8>>,
}

impl StampAnnotation {
    pub(crate) fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Self, AnnotationError> {
        let name = dictionary.optional_name(b"Name", objects)?.map(Vec::from);
        Ok(Self { name })
    }
}
