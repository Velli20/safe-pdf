use pdf_object_reader::{dictionary::Dictionary, object_resolver::ObjectResolver};

use crate::AnnotationError;

/// A generic optional-content wrapper.
pub struct OptionalContent {
    /// The original optional-content dictionary.
    pub dictionary: Dictionary,
}

impl OptionalContent {
    pub(crate) fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Option<Self>, AnnotationError> {
        dictionary
            .get(b"OC")
            .map(|value| {
                Ok(Self {
                    dictionary: value.try_dictionary(objects)?.clone(),
                })
            })
            .transpose()
    }
}
