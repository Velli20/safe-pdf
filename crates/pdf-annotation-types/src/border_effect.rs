use pdf_object_reader::{
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
        key: &'static [u8],
        objects: &dyn ObjectResolver,
    ) -> Result<Option<Self>, AnnotationError> {
        let Some(value) = dictionary.get(key) else {
            return Ok(None);
        };

        let dictionary = value.try_dictionary(objects)?;
        let style = dictionary
            .get(b"S")
            .map(|value| value.try_bytes(objects).map(BorderEffectStyle::from))
            .transpose()?;
        let intensity = dictionary.optional_number::<f32>(b"I", objects)?;

        Ok(Some(Self { style, intensity }))
    }
}
