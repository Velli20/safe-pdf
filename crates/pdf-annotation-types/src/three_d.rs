use pdf_object_reader::{dictionary::Dictionary, object_resolver::ObjectResolver};

use crate::AnnotationError;

/// A 3D view wrapper.
pub struct ThreeDView {
    /// The original 3D view dictionary.
    pub dictionary: Dictionary,
}

/// A file activation wrapper.
pub struct MovieActivation {
    /// The original activation dictionary.
    pub dictionary: Dictionary,
}

/// A 3D annotation wrapper.
pub struct ThreeDAnnotation {
    /// The default 3D view.
    pub default_view: Option<ThreeDView>,
    /// The 3D activation dictionary.
    pub activation: Option<MovieActivation>,
    /// The original 3D dictionary.
    pub dictionary: Dictionary,
}

impl ThreeDAnnotation {
    pub(crate) fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Option<Self>, AnnotationError> {
        let default_view = dictionary
            .get(b"3DV")
            .map(|value| {
                Ok::<ThreeDView, AnnotationError>(ThreeDView {
                    dictionary: value.try_dictionary(objects)?.clone(),
                })
            })
            .transpose()?;
        let activation = dictionary
            .get(b"3DA")
            .map(|value| {
                Ok::<MovieActivation, AnnotationError>(MovieActivation {
                    dictionary: value.try_dictionary(objects)?.clone(),
                })
            })
            .transpose()?;

        Ok(Some(Self {
            default_view,
            activation,
            dictionary: dictionary.clone(),
        }))
    }
}
