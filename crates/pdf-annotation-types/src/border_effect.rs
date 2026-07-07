use pdf_object::{
    dictionary::Dictionary, object_lookup::ObjectLookupExt, object_resolver::ObjectResolver,
};

use crate::{AnnotationError, BorderEffectStyle};

/// A border effect dictionary.
pub struct BorderEffect {
    /// The effect style.
    pub style: Option<BorderEffectStyle>,
    /// The intensity.
    pub intensity: Option<f32>,
}

impl BorderEffect {
    pub(crate) fn from_dictionary(
        dictionary: &Dictionary,
        key: &'static str,
        objects: &dyn ObjectResolver,
    ) -> Result<Option<Self>, AnnotationError> {
        let Some(value) = dictionary.get(key) else {
            return Ok(None);
        };

        let dictionary = value.try_dictionary(objects)?;
        let style = dictionary
            .get("S")
            .map(|value| {
                value
                    .try_str(objects)
                    .map(|name| BorderEffectStyle::from(name.as_ref()))
            })
            .transpose()?;
        let intensity = dictionary.optional_number::<f32>("I", objects)?;

        Ok(Some(Self { style, intensity }))
    }
}
