use pdf_object_reader::{
    dictionary::Dictionary, object_lookup::ObjectLookupExt, object_resolver::ObjectResolver,
};

use crate::{AnnotationError, CaretSymbolStyle};

/// Annotation-specific caret state.
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
        let difference_rect = dictionary.optional_array_of::<f32, 4>(b"RD", objects)?;
        let style = dictionary
            .get(b"Sy")
            .map(|value| value.try_bytes(objects).map(CaretSymbolStyle::from))
            .transpose()?;

        Ok(Self {
            difference_rect,
            style,
        })
    }
}
