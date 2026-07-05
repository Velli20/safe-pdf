use pdf_object::{dictionary::Dictionary, object_resolver::ObjectResolver};

use crate::{AnnotationError, helpers};

/// An appearance dictionary.
#[derive(Debug, Clone, PartialEq)]
pub struct AppearanceDictionary {
    /// The normal appearance.
    pub normal: Option<Dictionary>,
    /// The rollover appearance.
    pub rollover: Option<Dictionary>,
    /// The down appearance.
    pub down: Option<Dictionary>,
    /// The original appearance dictionary.
    pub dictionary: Dictionary,
}

impl AppearanceDictionary {
    pub(crate) fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Option<Self>, AnnotationError> {
        let Some(value) = dictionary.get("AP") else {
            return Ok(None);
        };

        let dictionary = value.try_dictionary(objects)?.clone();
        let normal = dictionary
            .get("N")
            .map(|value| helpers::dictionary(value, objects))
            .transpose()?;
        let rollover = dictionary
            .get("R")
            .map(|value| helpers::dictionary(value, objects))
            .transpose()?;
        let down = dictionary
            .get("D")
            .map(|value| helpers::dictionary(value, objects))
            .transpose()?;

        Ok(Some(Self {
            normal,
            rollover,
            down,
            dictionary,
        }))
    }
}
