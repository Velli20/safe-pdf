use pdf_annotation_types::FreeTextAlignment;
use pdf_font::{BaseEncoding, standard14::Standard14Font};
use pdf_graphics::{color::Color, rect::Rect};

/// Complete layout and appearance settings for generated free text.
#[derive(Clone, Debug, PartialEq)]
pub struct FreeTextStyle {
    /// Font used by the appearance stream.
    pub font: FreeTextFont,
    /// Font size in user-space points.
    pub font_size: f32,
    /// Baseline distance between adjacent lines.
    pub line_height: f32,
    /// Opaque RGB text color.
    pub text_color: Color,
    /// Optional opaque RGB background color.
    pub background_color: Option<Color>,
    /// Optional solid border.
    pub border: Option<FreeTextBorder>,
    /// Insets between the annotation bounds and its text.
    pub insets: Rect,
    /// Horizontal line alignment.
    pub alignment: FreeTextAlignment,
    /// Behavior when text exceeds the requested rectangle.
    pub overflow: FreeTextOverflow,
}

impl Default for FreeTextStyle {
    /// Returns a portable Standard 14 appearance suitable for editing.
    fn default() -> Self {
        Self {
            font: FreeTextFont {
                standard14: Standard14Font::Helvetica,
                resource_name: Vec::from(b"Helv"),
                encoding: BaseEncoding::WinAnsi,
            },
            font_size: 12.0,
            line_height: 14.4,
            text_color: Color::from_rgb(0.0, 0.0, 0.0),
            background_color: None,
            border: None,
            insets: Rect {
                left: 2.0,
                top: 2.0,
                right: 2.0,
                bottom: 2.0,
            },
            alignment: FreeTextAlignment::Left,
            overflow: FreeTextOverflow::ExpandRight,
        }
    }
}

/// A portable Standard 14 font and its appearance resource name.
#[derive(Clone, Debug, PartialEq)]
pub struct FreeTextFont {
    /// Standard 14 font used by the appearance.
    pub standard14: Standard14Font,
    /// Name used by `/DA` and the appearance resources.
    pub resource_name: Vec<u8>,
    /// Encoding used by text-showing operators.
    pub encoding: BaseEncoding,
}

/// Border styling for generated free text.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FreeTextBorder {
    /// Opaque RGB border color.
    pub color: Color,
    /// Border width in user-space points.
    pub width: f32,
}

/// Policy applied when text does not fit its requested rectangle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FreeTextOverflow {
    /// Increase the annotation height to contain wrapped lines.
    ExpandHeight,
    /// Disable word wrapping and increase the annotation width.
    ExpandRight,
    /// Reject text that does not fit the requested rectangle.
    Reject,
}
