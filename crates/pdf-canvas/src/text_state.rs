use pdf_font::font::Font;
use pdf_graphics::transform::Transform;
use pdf_page::resources::Resources;

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
    /// The current resource dictionary, used for resolving nested resources in Type 3 fonts.
    pub(crate) resources: Option<&'a Resources>,
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
            resources: None,
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

    /// Builds the glyph base transform ("S" matrix, ISO 32000 §9.4.4) from
    /// the current text state fields.
    ///
    /// - `units_per_em_inv`: reciprocal of the font's design units per em.
    ///   Pass `0.001` for Type1 (1000 u/em), `1.0 / units_per_em` for TrueType,
    ///   or `1.0` for Type3 (the font matrix handles design-unit scaling).
    pub(crate) fn glyph_base_transform(&self, units_per_em_inv: f32) -> Transform {
        let scale = self.font_size * units_per_em_inv;
        Transform::from_row(
            scale * self.horizontal_scaling,
            0.0,
            0.0,
            scale,
            0.0,
            self.rise,
        )
    }

    /// Concatenates the text matrix (Tm) and the CTM onto `base`, returning
    /// the final device-space glyph matrix.
    pub(crate) fn compose_glyph_matrix(&self, mut base: Transform, ctm: &Transform) -> Transform {
        base.concat(&self.matrix);
        base.concat(ctm);
        base
    }

    /// Advances the text cursor by one horizontal glyph (Type1/TrueType convention:
    /// glyph width in 1/1000 em units, scaled by font size).
    pub(crate) fn advance_horizontal_glyph(&mut self, char_code: u16) {
        let glyph_width_x = self.glyph_width(char_code) / 1000.0 * self.font_size;
        self.advance_text_cursor(char_code, glyph_width_x, 0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_text_state(font_size: f32, horizontal_scaling: f32, rise: f32) -> TextState<'static> {
        TextState {
            font_size,
            horizontal_scaling,
            rise,
            ..Default::default()
        }
    }

    fn transform_approx_eq(a: &Transform, b: &Transform, eps: f32) -> bool {
        (a.sx - b.sx).abs() <= eps
            && (a.ky - b.ky).abs() <= eps
            && (a.kx - b.kx).abs() <= eps
            && (a.sy - b.sy).abs() <= eps
            && (a.tx - b.tx).abs() <= eps
            && (a.ty - b.ty).abs() <= eps
    }

    #[test]
    fn glyph_base_transform_type1() {
        let ts = make_text_state(12.0, 1.0, 0.0);
        let m = ts.glyph_base_transform(0.001);
        assert_eq!(m, Transform::from_row(0.012, 0.0, 0.0, 0.012, 0.0, 0.0));
    }

    #[test]
    fn glyph_base_transform_with_scaling_and_rise() {
        let ts = make_text_state(10.0, 0.5, 2.0);
        let m = ts.glyph_base_transform(0.001);
        let expected = Transform::from_row(0.005, 0.0, 0.0, 0.01, 0.0, 2.0);
        assert!(
            transform_approx_eq(&m, &expected, 1e-6),
            "expected {expected:?}, got {m:?}"
        );
    }

    #[test]
    fn glyph_base_transform_type3_k1() {
        let ts = make_text_state(12.0, 1.0, 0.0);
        let m = ts.glyph_base_transform(1.0);
        assert_eq!(m, Transform::from_row(12.0, 0.0, 0.0, 12.0, 0.0, 0.0));
    }

    #[test]
    fn compose_glyph_matrix_identity() {
        let ts = make_text_state(12.0, 1.0, 0.0); // matrix = identity
        let base = Transform::identity();
        let ctm = Transform::identity();
        let result = ts.compose_glyph_matrix(base, &ctm);
        assert_eq!(result, Transform::identity());
    }

    #[test]
    fn compose_glyph_matrix_ordering() {
        // base = scale(2, 2), Tm = translate(3, 0), ctm = identity
        // Expected: concat(scale(2,2), translate(3,0)) = scale(2,2) with tx=3
        let mut ts = make_text_state(1.0, 1.0, 0.0);
        ts.matrix = Transform::from_translate(3.0, 0.0);
        let base = Transform::from_scale(2.0, 2.0);
        let ctm = Transform::identity();
        let result = ts.compose_glyph_matrix(base, &ctm);
        assert_eq!(result, Transform::from_row(2.0, 0.0, 0.0, 2.0, 3.0, 0.0));
    }

    #[test]
    fn advance_horizontal_glyph_no_font_is_noop() {
        // With no font, glyph_width returns 0.0, so the matrix should not move.
        let mut ts = make_text_state(12.0, 1.0, 0.0);
        let initial_matrix = ts.matrix;
        ts.advance_horizontal_glyph(0x41); // 'A'
        assert_eq!(ts.matrix, initial_matrix);
    }
}
