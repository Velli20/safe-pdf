use pdf_object::{
    dictionary::Dictionary, object_lookup::ObjectLookupExt, object_resolver::ObjectResolver,
};

use crate::{AnnotationError, BorderEffect};

/// Horizontal alignment for generated free text appearances.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum FreeTextAlignment {
    /// Align lines with the left edge of the padded content area.
    #[default]
    Left,
    /// Center lines within the padded content area.
    Center,
    /// Align lines with the right edge of the padded content area.
    Right,
}

impl FreeTextAlignment {
    /// Converts a PDF `/Q` value into a supported alignment.
    pub const fn from_quadding(value: i32) -> Option<Self> {
        match value {
            0 => Some(Self::Left),
            1 => Some(Self::Center),
            2 => Some(Self::Right),
            _ => None,
        }
    }

    /// Returns the PDF `/Q` value for this alignment.
    pub const fn quadding(self) -> i32 {
        match self {
            Self::Left => 0,
            Self::Center => 1,
            Self::Right => 2,
        }
    }
}

/// Annotation-specific free text state.
pub struct FreeTextAnnotation {
    /// The default appearance string.
    pub default_appearance: Option<Vec<u8>>,
    /// The quadding mode.
    pub quadding: Option<i32>,
    /// Rich text contents.
    pub rich_contents: Option<Vec<u8>>,
    /// The default style string.
    pub default_style: Option<Vec<u8>>,
    /// The callout line.
    pub callout_line: Option<Vec<f32>>,
    /// The border effect.
    pub border_effect: Option<BorderEffect>,
    /// The difference rectangle.
    pub difference_rect: Option<[f32; 4]>,
    /// The intent.
    pub intent: Option<Vec<u8>>,
}

impl FreeTextAnnotation {
    pub(crate) fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Self, AnnotationError> {
        let default_appearance = dictionary.optional_bytes_vec(b"DA", objects)?;
        let quadding = dictionary.optional_number::<i32>(b"Q", objects)?;
        let rich_contents = dictionary.optional_bytes_vec(b"RC", objects)?;
        let default_style = dictionary.optional_bytes_vec(b"DS", objects)?;
        let callout_line = dictionary.optional_vec_of::<f32>(b"CL", objects)?;
        let difference_rect = dictionary.optional_array_of::<f32, 4>(b"RD", objects)?;
        let intent = dictionary.optional_bytes(b"IT", objects)?.map(Vec::from);
        let border_effect = BorderEffect::from_dictionary(dictionary, b"BE", objects)?;

        Ok(Self {
            default_appearance,
            quadding,
            rich_contents,
            default_style,
            callout_line,
            border_effect,
            difference_rect,
            intent,
        })
    }
}
