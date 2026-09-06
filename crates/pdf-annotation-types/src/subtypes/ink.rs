use pdf_object_reader::{dictionary::Dictionary, object_resolver::ObjectResolver};

use crate::{AnnotationColor, AnnotationError, BorderStyle, InkList};

/// Annotation-specific ink state.
pub struct InkAnnotation {
    /// The required ink list.
    pub ink_list: InkList,
    /// The border style.
    pub border_style: Option<BorderStyle>,
    /// The interior color.
    pub interior_color: Option<AnnotationColor>,
}

impl InkAnnotation {
    pub(crate) fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Self, AnnotationError> {
        let ink_list = InkList::from_dictionary(dictionary, objects)?
            .ok_or(AnnotationError::MissingEntry { entry: b"InkList" })?;
        let border_style = BorderStyle::from_dictionary(dictionary, b"BS", objects)?;
        let interior_color = AnnotationColor::from_dictionary(dictionary, b"IC", objects)?;

        Ok(Self {
            ink_list,
            border_style,
            interior_color,
        })
    }
}
