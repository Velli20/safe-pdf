use pdf_object::{dictionary::Dictionary, object_resolver::ObjectResolver};

use crate::{AnnotationError, CaretSymbolStyle};

/// Annotation-specific caret state.
#[derive(Debug, Clone, PartialEq)]
pub struct CaretAnnotation {
    /// The difference rectangle.
    pub difference_rect: Option<[f32; 4]>,
    /// The caret style.
    pub style: Option<CaretSymbolStyle>,
}

impl CaretAnnotation {
    pub(crate) fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Self, AnnotationError> {
        let difference_rect = dictionary
            .get("RD")
            .map(|value| value.try_array_of::<f32, 4>(objects))
            .transpose()?;
        let style = dictionary
            .get("Sy")
            .map(|value| {
                value
                    .try_str(objects)
                    .map(|name| CaretSymbolStyle::from(name.as_ref()))
            })
            .transpose()?;

        Ok(Self {
            difference_rect,
            style,
        })
    }
}
