use pdf_object::{
    dictionary::Dictionary, object_lookup::ObjectLookupExt, object_resolver::ObjectResolver,
};

use crate::AnnotationError;

/// Annotation-specific text state.
pub struct TextAnnotation {
    /// Whether the note popup starts open.
    pub open: Option<bool>,
    /// The note icon name.
    pub name: Option<Vec<u8>>,
    /// The annotation state.
    pub state: Option<Vec<u8>>,
    /// The annotation state model.
    pub state_model: Option<Vec<u8>>,
    /// The intent name.
    pub intent: Option<Vec<u8>>,
    /// Extension data.
    pub ex_data: Option<Dictionary>,
}

impl TextAnnotation {
    pub(crate) fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Self, AnnotationError> {
        let open = dictionary.optional_boolean(b"Open", objects)?;
        let name = dictionary.optional_bytes(b"Name", objects)?.map(Vec::from);
        let state = dictionary.optional_bytes_vec(b"State", objects)?;
        let state_model = dictionary.optional_bytes_vec(b"StateModel", objects)?;
        let intent = dictionary.optional_bytes(b"IT", objects)?.map(Vec::from);
        let ex_data = dictionary
            .get(b"ExData")
            .map(|value| crate::helpers::dictionary(value, objects))
            .transpose()?;

        Ok(Self {
            open,
            name,
            state,
            state_model,
            intent,
            ex_data,
        })
    }
}
