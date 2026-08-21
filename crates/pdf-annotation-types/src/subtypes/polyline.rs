use pdf_graphics::pdf_path::PdfPath;
use pdf_object::{
    dictionary::Dictionary, object_lookup::ObjectLookupExt, object_resolver::ObjectResolver,
};

use crate::{AnnotationColor, AnnotationError, BorderStyle, LineEndingStyle, helpers};

/// Annotation-specific polyline state.
pub struct PolyLineAnnotation {
    /// The required polyline vertices.
    pub vertices: PdfPath,
    /// The line ending styles.
    pub line_endings: Option<[LineEndingStyle; 2]>,
    /// The interior color.
    pub interior_color: Option<AnnotationColor>,
    /// The border style.
    pub border_style: Option<BorderStyle>,
    /// The intent.
    pub intent: Option<Vec<u8>>,
}

impl PolyLineAnnotation {
    pub(crate) fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Self, AnnotationError> {
        let vertices =
            helpers::point_list(b"Vertices", dictionary.get_or_err(b"Vertices")?, objects)?;
        let line_endings = super::line_endings(dictionary, objects)?;
        let interior_color = AnnotationColor::from_dictionary(dictionary, b"IC", objects)?;
        let border_style = BorderStyle::from_dictionary(dictionary, b"BS", objects)?;
        let intent = dictionary.optional_name(b"IT", objects)?.map(Vec::from);

        Ok(Self {
            vertices,
            line_endings,
            interior_color,
            border_style,
            intent,
        })
    }
}
