use pdf_object_reader::{
    dictionary::Dictionary, object_lookup::ObjectLookupExt, object_resolver::ObjectResolver,
};

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
            .get(b"Movie")
            .map(|value| helpers::dictionary(value, objects))
            .transpose()?;
        let title = dictionary.optional_bytes_vec(b"T", objects)?;

        Ok(Self { movie, title })
    }
}
