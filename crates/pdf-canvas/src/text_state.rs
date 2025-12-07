use pdf_font::font::Font;
use pdf_graphics::transform::Transform;

/// Encapsulates text-specific state parameters.
#[derive(Clone)]
pub(crate) struct TextState<'a> {
    /// The text matrix (Tm), transforming text space to user space.
    pub(crate) matrix: Transform,
    /// The text line matrix (Tlm), tracking the start of the current line.
    pub(crate) line_matrix: Transform,
    /// Horizontal scaling of text (Th), as a factor (default: 1.0).
    pub(crate) horizontal_scaling: f32,
    /// Font size (Tfs), in user space units.
    pub(crate) font_size: f32,
    /// Character spacing (Tc), in unscaled text space units.
    pub(crate) character_spacing: f32,
    /// Word spacing (Tw), in unscaled text space units.
    pub(crate) word_spacing: f32,
    /// Text rise (Ts), a vertical offset from the baseline, in unscaled text space units.
    pub(crate) rise: f32,
    /// Text leading (Tl), the vertical distance between baselines.
    pub(crate) leading: f32,
    /// The current font resource.
    pub(crate) font: Option<&'a Font>,
}

impl Default for TextState<'_> {
    fn default() -> Self {
        Self {
            matrix: Transform::identity(),
            line_matrix: Transform::identity(),
            horizontal_scaling: 1.0,
            font_size: 0.0,
            character_spacing: 0.0,
            word_spacing: 0.0,
            rise: 0.0,
            leading: 0.0,
            font: None,
        }
    }
}

impl TextState<'_> {
    pub(crate) fn glyph_width(&self, char_code: u16) -> f32 {
        if let Some(font) = self.font {
            font.get_glyph_width(char_code)
        } else {
            0.0
        }
    }

    pub(crate) fn glyph_name(&self, char_code: u16) -> Option<&str> {
        if let Some(font) = self.font {
            font.glyph_name(char_code)
        } else {
            None
        }
    }
}
