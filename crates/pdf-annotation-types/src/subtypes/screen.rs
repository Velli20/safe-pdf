use pdf_object_reader::{dictionary::Dictionary, object_resolver::ObjectResolver};

use crate::{
    AnnotationAction, AnnotationError, AppearanceCharacteristics, BorderEffect, BorderStyle,
    helpers,
};

/// Annotation-specific screen state.
pub struct ScreenAnnotation {
    /// The annotation action.
    pub action: Option<AnnotationAction>,
    /// The additional actions.
    pub additional_actions: Option<Dictionary>,
    /// The appearance characteristics.
    pub appearance_characteristics: Option<AppearanceCharacteristics>,
    /// The border style.
    pub border_style: Option<BorderStyle>,
    /// The border effect.
    pub border_effect: Option<BorderEffect>,
}

impl ScreenAnnotation {
    pub(crate) fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Self, AnnotationError> {
        let action = AnnotationAction::from_dictionary(dictionary, b"A", objects)?;
        let additional_actions = dictionary
            .get(b"AA")
            .map(|value| helpers::dictionary(value, objects))
            .transpose()?;
        let appearance_characteristics =
            AppearanceCharacteristics::from_dictionary(dictionary, objects)?;
        let border_style = BorderStyle::from_dictionary(dictionary, b"BS", objects)?;
        let border_effect = BorderEffect::from_dictionary(dictionary, b"BE", objects)?;

        Ok(Self {
            action,
            additional_actions,
            appearance_characteristics,
            border_style,
            border_effect,
        })
    }
}
