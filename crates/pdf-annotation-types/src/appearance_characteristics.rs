use pdf_object_reader::{
    dictionary::Dictionary, object_lookup::ObjectLookupExt, object_resolver::ObjectResolver,
};

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
        let Some(value) = dictionary.get(b"MK") else {
            return Ok(None);
        };

        let dictionary = value.try_dictionary(objects)?;
        let rotation = dictionary.optional_number::<i32>(b"R", objects)?;
        let border_color = AnnotationColor::from_dictionary(dictionary, b"BC", objects)?;
        let background_color = AnnotationColor::from_dictionary(dictionary, b"BG", objects)?;
        let normal_caption = dictionary.optional_bytes_vec(b"CA", objects)?;
        let rollover_caption = dictionary.optional_bytes_vec(b"RC", objects)?;
        let alternate_caption = dictionary.optional_bytes_vec(b"AC", objects)?;

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
