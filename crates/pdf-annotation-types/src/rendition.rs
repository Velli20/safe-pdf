use pdf_object::{dictionary::Dictionary, object_resolver::ObjectResolver};

use crate::AnnotationError;

/// A rendition dictionary wrapper.
pub struct Rendition {
    /// The original rendition dictionary.
    pub dictionary: Dictionary,
}

impl Rendition {
    pub(crate) fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Option<Self>, AnnotationError> {
        dictionary
            .get("R")
            .map(|value| {
                Ok(Self {
                    dictionary: value.try_dictionary(objects)?.clone(),
                })
            })
            .transpose()
    }
}
