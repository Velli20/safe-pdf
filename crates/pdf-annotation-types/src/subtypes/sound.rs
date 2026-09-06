use pdf_object_reader::{
    dictionary::Dictionary, object_lookup::ObjectLookupExt, object_resolver::ObjectResolver,
};

use crate::{AnnotationError, helpers};

/// Annotation-specific sound state.
pub struct SoundAnnotation {
    /// The optional sound dictionary or stream dictionary.
    pub sound: Option<Dictionary>,
    /// The sampling rate.
    pub rate: Option<f32>,
    /// The number of channels.
    pub channels: Option<i32>,
}

impl SoundAnnotation {
    pub(crate) fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Self, AnnotationError> {
        let sound = dictionary
            .get(b"Sound")
            .map(|value| helpers::dictionary(value, objects))
            .transpose()?;
        let rate = dictionary.optional_number::<f32>(b"R", objects)?;
        let channels = dictionary.optional_number::<i32>(b"C", objects)?;

        Ok(Self {
            sound,
            rate,
            channels,
        })
    }
}
