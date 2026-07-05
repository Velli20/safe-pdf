use pdf_object::{dictionary::Dictionary, object_resolver::ObjectResolver};

use crate::{AnnotationError, helpers};

/// Annotation-specific movie state.
pub struct MovieAnnotation {
    /// The movie data dictionary.
    pub movie: Option<Dictionary>,
    /// The title.
    pub title: Option<Vec<u8>>,
}

impl MovieAnnotation {
    pub(crate) fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Self, AnnotationError> {
        let movie = dictionary
            .get("Movie")
            .map(|value| helpers::dictionary(value, objects))
            .transpose()?;
        let title = dictionary
            .get("T")
            .map(|value| value.try_bytes_vec(objects))
            .transpose()?;

        Ok(Self { movie, title })
    }
}
