use pdf_object_reader::{
    dictionary::Dictionary, object_lookup::ObjectLookupExt, object_resolver::ObjectResolver,
};

use crate::{AnnotationColor, AnnotationError, QuadPoints};

/// Annotation-specific highlight state.
pub struct HighlightAnnotation {
    /// The quad points.
    pub quad_points: QuadPoints,
    /// The annotation color.
    pub color: Option<AnnotationColor>,
    /// The constant opacity.
    pub constant_opacity: Option<f32>,
}

impl HighlightAnnotation {
    pub(crate) fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Self, AnnotationError> {
        let quad_points = super::required_quad_points(dictionary, objects)?;
        let color = AnnotationColor::from_dictionary(dictionary, b"C", objects)?;
        let constant_opacity = dictionary.optional_number::<f32>(b"CA", objects)?;

        Ok(Self {
            quad_points,
            color,
            constant_opacity,
        })
    }
}
