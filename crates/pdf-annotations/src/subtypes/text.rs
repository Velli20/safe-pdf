use pdf_object::{dictionary::Dictionary, object_resolver::ObjectResolver};

use crate::AnnotationError;

/// Annotation-specific text state.
#[derive(Debug, Clone, PartialEq)]
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
        let open = dictionary
            .get("Open")
            .map(|value| value.try_boolean(objects))
            .transpose()?;
        let name = dictionary
            .get("Name")
            .map(|value| value.try_bytes_vec(objects))
            .transpose()?;
        let state = dictionary
            .get("State")
            .map(|value| value.try_bytes_vec(objects))
            .transpose()?;
        let state_model = dictionary
            .get("StateModel")
            .map(|value| value.try_bytes_vec(objects))
            .transpose()?;
        let intent = dictionary
            .get("IT")
            .map(|value| value.try_bytes_vec(objects))
            .transpose()?;
        let ex_data = dictionary
            .get("ExData")
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
