use pdf_object::{
    dictionary::Dictionary, object_lookup::ObjectLookupExt, object_resolver::ObjectResolver,
};

use crate::AnnotationError;

/// Annotation-specific popup state.
pub struct PopupAnnotation {
    /// The required parent annotation reference.
    pub parent: Option<usize>,
    /// Whether the popup is open.
    pub open: Option<bool>,
}

impl PopupAnnotation {
    pub(crate) fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Self, AnnotationError> {
        let parent = dictionary
            .get("Parent")
            .map(|obj| obj.try_object_number())
            .transpose()?;
        let open = dictionary.optional_boolean("Open", objects)?;

        Ok(Self { parent, open })
    }
}
