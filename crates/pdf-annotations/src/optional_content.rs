use pdf_object::{dictionary::Dictionary, object_resolver::ObjectResolver};

use crate::AnnotationError;

/// A generic optional-content wrapper.
#[derive(Debug, Clone, PartialEq)]
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
            .get("OC")
            .map(|value| {
                Ok(Self {
                    dictionary: value.try_dictionary(objects)?.clone(),
                })
            })
            .transpose()
    }
}
