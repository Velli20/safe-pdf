use pdf_object_reader::{
    dictionary::Dictionary, object_lookup::ObjectLookupExt, object_resolver::ObjectResolver,
};

use crate::{AnnotationColor, AnnotationError, BorderStyle, LineEndingStyle};

/// Annotation-specific line state.
pub struct LineAnnotation {
    /// The required line endpoints.
    pub line: [f32; 4],
    /// The line ending styles.
    pub line_endings: Option<[LineEndingStyle; 2]>,
    /// The border style.
    pub border_style: Option<BorderStyle>,
    /// The interior color.
    pub interior_color: Option<AnnotationColor>,
    /// The leader line length.
    pub leader_line_length: Option<f32>,
    /// The leader line extension length.
    pub leader_line_extension: Option<f32>,
    /// Whether the caption is positioned at the line's end.
    pub caption: Option<bool>,
    /// The intent.
    pub intent: Option<Vec<u8>>,
}

impl LineAnnotation {
    pub(crate) fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Self, AnnotationError> {
        let line = dictionary
            .get_or_err(b"L")?
            .try_array_of::<f32, 4>(objects)?;
        let line_endings = super::line_endings(dictionary, objects)?;
        let border_style = BorderStyle::from_dictionary(dictionary, b"BS", objects)?;
        let interior_color = AnnotationColor::from_dictionary(dictionary, b"IC", objects)?;
        let leader_line_length = dictionary.optional_number::<f32>(b"LL", objects)?;
        let leader_line_extension = dictionary.optional_number::<f32>(b"LLE", objects)?;
        let caption = dictionary.optional_boolean(b"Cap", objects)?;
        let intent = dictionary.optional_bytes(b"IT", objects)?.map(Vec::from);

        Ok(Self {
            line,
            line_endings,
            border_style,
            interior_color,
            leader_line_length,
            leader_line_extension,
            caption,
            intent,
        })
    }
}
