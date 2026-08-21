use pdf_object::{
    dictionary::Dictionary, object_lookup::ObjectLookupExt, object_resolver::ObjectResolver,
};

use crate::{AnnotationColor, AnnotationError, BorderEffect, BorderStyle};

/// Annotation-specific circle state.
pub struct CircleAnnotation {
    /// The border style.
    pub border_style: Option<BorderStyle>,
    /// The interior color.
    pub interior_color: Option<AnnotationColor>,
    /// The border effect.
    pub border_effect: Option<BorderEffect>,
    /// The difference rectangle.
    pub difference_rect: Option<[f32; 4]>,
}

impl CircleAnnotation {
    pub(crate) fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Self, AnnotationError> {
        let border_style = BorderStyle::from_dictionary(dictionary, b"BS", objects)?;
        let interior_color = AnnotationColor::from_dictionary(dictionary, b"IC", objects)?;
        let border_effect = BorderEffect::from_dictionary(dictionary, b"BE", objects)?;
        let difference_rect = dictionary.optional_array_of::<f32, 4>(b"RD", objects)?;

        Ok(Self {
            border_style,
            interior_color,
            border_effect,
            difference_rect,
        })
    }
}
