use pdf_object::{dictionary::Dictionary, object_resolver::ObjectResolver};

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
            .get_or_err("L")?
            .try_array_of::<f32, 4>(objects)?;
        let line_endings = super::line_endings(dictionary, objects)?;
        let border_style = BorderStyle::from_dictionary(dictionary, "BS", objects)?;
        let interior_color = AnnotationColor::from_dictionary(dictionary, "IC", objects)?;
        let leader_line_length = dictionary
            .get("LL")
            .map(|value| value.try_number::<f32>(objects))
            .transpose()?;
        let leader_line_extension = dictionary
            .get("LLE")
            .map(|value| value.try_number::<f32>(objects))
            .transpose()?;
        let caption = dictionary
            .get("Cap")
            .map(|value| value.try_boolean(objects))
            .transpose()?;
        let intent = dictionary
            .get("IT")
            .map(|value| value.try_bytes_vec(objects))
            .transpose()?;

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
