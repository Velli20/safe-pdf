use pdf_object::{
    dictionary::Dictionary, object_lookup::ObjectLookupExt, object_resolver::ObjectResolver,
};

use crate::{AnnotationColor, AnnotationError, QuadPoints};

/// Annotation-specific strikeout state.
pub struct StrikeOutAnnotation {
    /// The quad points.
    pub quad_points: QuadPoints,
    /// The annotation color.
    pub color: Option<AnnotationColor>,
    /// The constant opacity.
    pub constant_opacity: Option<f32>,
}

impl StrikeOutAnnotation {
    pub(crate) fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Self, AnnotationError> {
        let quad_points = super::required_quad_points(dictionary, objects)?;
        let color = AnnotationColor::from_dictionary(dictionary, "C", objects)?;
        let constant_opacity = dictionary.optional_number::<f32>("CA", objects)?;

        Ok(Self {
            quad_points,
            color,
            constant_opacity,
        })
    }
}
