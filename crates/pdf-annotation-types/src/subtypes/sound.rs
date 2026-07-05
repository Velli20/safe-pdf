use pdf_object::{dictionary::Dictionary, object_resolver::ObjectResolver};

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
            .get("Sound")
            .map(|value| helpers::dictionary(value, objects))
            .transpose()?;
        let rate = dictionary
            .get("R")
            .map(|value| value.try_number::<f32>(objects))
            .transpose()?;
        let channels = dictionary
            .get("C")
            .map(|value| value.try_number::<i32>(objects))
            .transpose()?;

        Ok(Self {
            sound,
            rate,
            channels,
        })
    }
}
