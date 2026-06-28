use pdf_object::{dictionary::Dictionary, object_resolver::ObjectResolver};

use crate::AnnotationError;

/// Annotation-specific popup state.
#[derive(Debug, Clone, PartialEq)]
pub struct PopupAnnotation {
    /// The required parent annotation reference.
    pub parent: usize,
    /// Whether the popup is open.
    pub open: Option<bool>,
}

impl PopupAnnotation {
    pub(crate) fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Self, AnnotationError> {
        let parent = dictionary.get_or_err("Parent")?.try_object_number()?;
        let open = dictionary
            .get("Open")
            .map(|value| value.try_boolean(objects))
            .transpose()?;

        Ok(Self { parent, open })
    }
}
