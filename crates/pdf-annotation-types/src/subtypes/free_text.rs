use pdf_object::{
    dictionary::Dictionary, object_lookup::ObjectLookupExt, object_resolver::ObjectResolver,
};

use crate::{AnnotationError, BorderEffect};

/// Annotation-specific free text state.
pub struct FreeTextAnnotation {
    /// The default appearance string.
    pub default_appearance: Option<Vec<u8>>,
    /// The quadding mode.
    pub quadding: Option<i32>,
    /// Rich text contents.
    pub rich_contents: Option<Vec<u8>>,
    /// The default style string.
    pub default_style: Option<Vec<u8>>,
    /// The callout line.
    pub callout_line: Option<Vec<f32>>,
    /// The border effect.
    pub border_effect: Option<BorderEffect>,
    /// The difference rectangle.
    pub difference_rect: Option<[f32; 4]>,
    /// The intent.
    pub intent: Option<Vec<u8>>,
}

impl FreeTextAnnotation {
    pub(crate) fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Self, AnnotationError> {
        let default_appearance = dictionary.optional_bytes_vec("DA", objects)?;
        let quadding = dictionary.optional_number::<i32>("Q", objects)?;
        let rich_contents = dictionary.optional_bytes_vec("RC", objects)?;
        let default_style = dictionary.optional_bytes_vec("DS", objects)?;
        let callout_line = dictionary.optional_vec_of::<f32>("CL", objects)?;
        let difference_rect = dictionary.optional_array_of::<f32, 4>("RD", objects)?;
        let intent = dictionary.optional_bytes_vec("IT", objects)?;
        let border_effect = BorderEffect::from_dictionary(dictionary, "BE", objects)?;

        Ok(Self {
            default_appearance,
            quadding,
            rich_contents,
            default_style,
            callout_line,
            border_effect,
            difference_rect,
            intent,
        })
    }
}
