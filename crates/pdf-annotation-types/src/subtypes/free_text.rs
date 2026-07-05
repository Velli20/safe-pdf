use pdf_object::{dictionary::Dictionary, object_resolver::ObjectResolver};

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
        let default_appearance = dictionary
            .get("DA")
            .map(|value| value.try_bytes_vec(objects))
            .transpose()?;
        let quadding = dictionary
            .get("Q")
            .map(|value| value.try_number::<i32>(objects))
            .transpose()?;
        let rich_contents = dictionary
            .get("RC")
            .map(|value| value.try_bytes_vec(objects))
            .transpose()?;
        let default_style = dictionary
            .get("DS")
            .map(|value| value.try_bytes_vec(objects))
            .transpose()?;
        let callout_line = dictionary
            .get("CL")
            .map(|value| value.try_vec_of::<f32>(objects))
            .transpose()?;
        let difference_rect = dictionary
            .get("RD")
            .map(|value| value.try_array_of::<f32, 4>(objects))
            .transpose()?;
        let intent = dictionary
            .get("IT")
            .map(|value| value.try_bytes_vec(objects))
            .transpose()?;
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
