use pdf_object::{dictionary::Dictionary, object_resolver::ObjectResolver};

use crate::{AnnotationColor, AnnotationError};

/// Appearance characteristics from a `/MK` dictionary.
pub struct AppearanceCharacteristics {
    /// The rotation in degrees.
    pub rotation: Option<i32>,
    /// The border color.
    pub border_color: Option<AnnotationColor>,
    /// The background color.
    pub background_color: Option<AnnotationColor>,
    /// The normal caption.
    pub normal_caption: Option<Vec<u8>>,
    /// The rollover caption.
    pub rollover_caption: Option<Vec<u8>>,
    /// The alternate caption.
    pub alternate_caption: Option<Vec<u8>>,
}

impl AppearanceCharacteristics {
    pub(crate) fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Option<Self>, AnnotationError> {
        let Some(value) = dictionary.get("MK") else {
            return Ok(None);
        };

        let dictionary = value.try_dictionary(objects)?;
        let rotation = dictionary
            .get("R")
            .map(|value| value.try_number::<i32>(objects))
            .transpose()?;
        let border_color = AnnotationColor::from_dictionary(dictionary, "BC", objects)?;
        let background_color = AnnotationColor::from_dictionary(dictionary, "BG", objects)?;
        let normal_caption = dictionary
            .get("CA")
            .map(|value| value.try_bytes_vec(objects))
            .transpose()?;
        let rollover_caption = dictionary
            .get("RC")
            .map(|value| value.try_bytes_vec(objects))
            .transpose()?;
        let alternate_caption = dictionary
            .get("AC")
            .map(|value| value.try_bytes_vec(objects))
            .transpose()?;

        Ok(Some(Self {
            rotation,
            border_color,
            background_color,
            normal_caption,
            rollover_caption,
            alternate_caption,
        }))
    }
}
