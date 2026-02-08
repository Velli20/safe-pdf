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

    /// Advances the text matrix after rendering a glyph.
    ///
    /// Applies the standard PDF text advance formula, which computes the new text
    /// position based on the glyph's width and the current text state parameters:
    /// - `advance_x = (glyph_width_x + Tc + Tw_if_space) × Th`
    /// - `advance_y = glyph_width_y`
    ///
    /// Word spacing (`Tw`) is applied only to the space character (char code 0x20).
    ///
    /// # Parameters
    ///
    /// - `char_code`: The character code of the glyph just rendered.
    /// - `glyph_width_x`: The horizontal glyph displacement, already scaled to
    ///   text-space units (e.g. `w0 / 1000 × Tfs` for Type1/TrueType).
    /// - `glyph_width_y`: The vertical glyph displacement in text-space units
    ///   (0.0 for horizontal writing modes).
    pub(crate) fn advance_text_cursor(
        &mut self,
        char_code: u16,
        glyph_width_x: f32,
        glyph_width_y: f32,
    ) {
        const SPACE_CHAR_CODE: u16 = 0x20;

        let word_spacing_for_char = if char_code == SPACE_CHAR_CODE {
            self.word_spacing
        } else {
            0.0
        };
        let advance_x = (glyph_width_x + self.character_spacing + word_spacing_for_char)
            * self.horizontal_scaling;
        self.matrix.post_translate(advance_x, glyph_width_y);
    }
}
