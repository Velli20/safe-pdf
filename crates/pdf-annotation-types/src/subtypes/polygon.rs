use pdf_graphics::pdf_path::PdfPath;
use pdf_object::{dictionary::Dictionary, object_resolver::ObjectResolver};

use crate::{AnnotationColor, AnnotationError, BorderStyle, LineEndingStyle, helpers};

/// Annotation-specific polygon state.
pub struct PolygonAnnotation {
    /// The required polygon vertices.
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

impl PolygonAnnotation {
    pub(crate) fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Self, AnnotationError> {
        let mut vertices =
            helpers::point_list("Vertices", dictionary.get_or_err("Vertices")?, objects)?;
        vertices.close();

        let line_endings = super::line_endings(dictionary, objects)?;
        let interior_color = AnnotationColor::from_dictionary(dictionary, "IC", objects)?;
        let border_style = BorderStyle::from_dictionary(dictionary, "BS", objects)?;
        let intent = dictionary
            .get("IT")
            .map(|value| value.try_bytes_vec(objects))
            .transpose()?;

        Ok(Self {
            vertices,
            line_endings,
            interior_color,
            border_style,
            intent,
        })
    }
}
