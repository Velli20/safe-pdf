use pdf_object::{dictionary::Dictionary, object_resolver::ObjectResolver};

use crate::{
    AnnotationAction, AnnotationDestination, AnnotationError, BorderEffect, BorderStyle,
    LinkHighlightMode, QuadPoints,
};

/// Annotation-specific link state.
pub struct LinkAnnotation {
    /// The link highlight mode.
    pub highlight_mode: Option<LinkHighlightMode>,
    /// The annotation destination.
    pub destination: Option<AnnotationDestination>,
    /// The annotation action.
    pub action: Option<AnnotationAction>,
    /// The quad points.
    pub quad_points: Option<QuadPoints>,
    /// The border style.
    pub border_style: Option<BorderStyle>,
    /// The border effect.
    pub border_effect: Option<BorderEffect>,
}

impl LinkAnnotation {
    pub(crate) fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Self, AnnotationError> {
        let highlight_mode = dictionary
            .get("H")
            .map(|value| value.try_str(objects).map(LinkHighlightMode::from))
            .transpose()?;
        let destination = AnnotationDestination::from_dictionary(dictionary, "Dest", objects)?;
        let action = AnnotationAction::from_dictionary(dictionary, "A", objects)?;
        let quad_points = QuadPoints::from_dictionary(dictionary, objects)?;
        let border_style = BorderStyle::from_dictionary(dictionary, "BS", objects)?;
        let border_effect = BorderEffect::from_dictionary(dictionary, "BE", objects)?;

        Ok(Self {
            highlight_mode,
            destination,
            action,
            quad_points,
            border_style,
            border_effect,
        })
    }
}
