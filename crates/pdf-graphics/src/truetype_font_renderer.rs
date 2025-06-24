use pdf_content_stream::pdf_operator_backend::PdfOperatorBackend;
use pdf_font::font::Font;
use pdf_object::ObjectVariant;

use crate::{
    PathFillType, canvas::Canvas, error::PdfCanvasError, pdf_path::PdfPath,
    text_renderer::TextRenderer, transform::Transform,
};
use ttf_parser::{Face, GlyphId, OutlineBuilder};

/// A text renderer for TrueType-based fonts.
pub(crate) struct TrueTypeFontRenderer<'a, T: PdfOperatorBackend + Canvas> {
    /// The canvas backend where glyphs are drawn.
    canvas: &'a mut T,
    /// The font definition, containing glyph data, metrics, and character maps.
    font: &'a Font,
    /// The current text matrix (Tm), which positions the text.
    text_matrix: Transform,
    /// The Current Transformation Matrix (CTM) at the time of rendering.
    current_transform: Transform,
    /// The font size in user space units.
    font_size: f32,
    /// The text rise (Ts), a vertical offset from the baseline.
    rise: f32,
    /// The spacing to add between words, applied to space characters.
    word_spacing: f32,
    /// The spacing to add between individual characters.
    char_spacing: f32,
    /// The horizontal scaling factor for glyphs, as a percentage [0-100].
    horizontal_scaling: f32,
}

impl<'a, T: PdfOperatorBackend + Canvas> TrueTypeFontRenderer<'a, T> {
    pub fn new(
        canvas: &'a mut T,
        font: &'a Font,
        font_size: f32,
        horizontal_scaling: f32,
        text_matrix: Transform,
        current_transform: Transform,
        rise: f32,
        word_spacing: f32,
        char_spacing: f32,
    ) -> Result<Self, PdfCanvasError> {
        Ok(Self {
            canvas,
            font,
            text_matrix,
            current_transform,
            font_size,
            rise,
            word_spacing,
            char_spacing,
            horizontal_scaling,
        })
    }
}

impl<'a, T: PdfOperatorBackend + Canvas> TextRenderer for TrueTypeFontRenderer<'a, T> {
    fn render_text(&mut self, text: &[u8]) -> Result<(), crate::error::PdfCanvasError> {
        let Some(cid_font) = &self.font.cid_font else {
            panic!()
        };

        let Some(font_file) = &cid_font.descriptor.font_file else {
            panic!()
        };

        let face = if let ObjectVariant::Stream(s) = &font_file {
            Face::parse(s.data.as_slice(), 0).expect("Failed to parse font face")
        } else {
            panic!()
        };

        // Extract font and text state parameters.
        let units_per_em_f32 = face.units_per_em() as f32;
        let char_spacing = self.char_spacing;
        let word_spacing = self.word_spacing;
        let text_rise = self.rise;

        // Compute the inverse of units per em for scaling.
        let upe_inv = if units_per_em_f32 != 0.0 {
            1.0 / units_per_em_f32
        } else {
            0.0
        };

        // Th_factor: Horizontal scaling factor (Th / 100).
        let th_factor = self.horizontal_scaling / 100.0;

        // Build the text rendering transform for this glyph:
        // - sx: horizontal scale (font size, units per em, horizontal scaling)
        // - sy: vertical scale (font size, units per em)
        // - ty: vertical offset (text rise)
        let m_params = Transform::from_row(
            self.font_size * upe_inv * th_factor, // sx
            0.0,                                  // ky (skew)
            0.0,                                  // kx (skew)
            self.font_size * upe_inv,             // sy
            0.0,                                  // tx
            text_rise,                            // ty
        );

        let mut iter = text.iter();
        if self.font.encoding.is_some() {
            let _ = iter.next();
        }

        let cmap = self
            .font
            .cmap
            .as_ref()
            .ok_or(PdfCanvasError::NoCharacterMapForFont(
                self.font.base_font.clone(),
            ))?;

        // Iterate over each character in the input text.
        while let Some(char_code_byte) = iter.next() {
            if self.font.encoding.is_some() {
                let _ = iter.next();
            }

            let char_code = *char_code_byte;

            let mut glyph_id = GlyphId(char_code as u16);

            // Compose the final transformation matrix for this glyph:
            // m_params -> text matrix -> current transformation matrix
            let mut glyph_matrix_for_char = m_params.clone();
            glyph_matrix_for_char.concat(&self.text_matrix);
            glyph_matrix_for_char.concat(&self.current_transform);

            // Build the glyph outline using the composed transform.
            let mut builder = PdfGlyphOutline::new(glyph_matrix_for_char);

            if let Some(a) = cmap.get_mapping(*char_code_byte as u32) {
                if let Some(x) = face.glyph_index(a) {
                    glyph_id = x;
                }
            }

            face.outline_glyph(glyph_id, &mut builder);

            // Fill it on the canvas
            self.canvas
                .fill_path(&builder.path, PathFillType::Winding)?;

            // Determine the glyph's advance width in font units.
            let w0_glyph_units = cid_font
                .widths
                .as_ref()
                .and_then(|w_array| w_array.get_width(char_code as i64))
                .unwrap_or_else(|| self.font.cid_font.as_ref().unwrap().default_width as f32);

            // Convert width from font units to ems.
            let w0_ems = w0_glyph_units / 1000.0;

            // Scale the glyph width by the font size.
            let glyph_width_tfs_scaled = w0_ems * self.font_size;

            // Apply word spacing only to space characters.
            let word_spacing_for_char = if char_code == 32 { word_spacing } else { 0.0 };

            // Compute the horizontal advance for this glyph.
            let advance_x =
                (glyph_width_tfs_scaled + char_spacing + word_spacing_for_char) * th_factor;

            // Advance the text matrix for the next glyph.
            self.text_matrix.translate(advance_x, 0.0);
        }

        Ok(())
    }
}

/// An implementation of `ttf_parser::OutlineBuilder` that converts glyph outlines
/// into a `PdfPath`.
#[derive(Default)]
pub struct PdfGlyphOutline {
    /// The `PdfPath` being constructed from the glyph outline commands.
    path: PdfPath,
    /// The transformation matrix to apply to each point of the glyph outline.
    transform: Transform,
}

impl PdfGlyphOutline {
    pub fn new(transform: Transform) -> Self {
        Self {
            path: PdfPath::default(),
            transform,
        }
    }
}

impl OutlineBuilder for PdfGlyphOutline {
    fn move_to(&mut self, x: f32, y: f32) {
        let (x, y) = self.transform.transform_point(x, y);
        self.path.move_to(x, y).unwrap();
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let (x, y) = self.transform.transform_point(x, y);
        self.path.line_to(x, y).unwrap();
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let (x1, y1) = self.transform.transform_point(x1, y1);
        let (x, y) = self.transform.transform_point(x, y);
        self.path.quad_to(x1, y1, x, y).unwrap()
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let (x1, y1) = self.transform.transform_point(x1, y1);
        let (x2, y2) = self.transform.transform_point(x2, y2);
        let (x, y) = self.transform.transform_point(x, y);
        self.path.curve_to(x1, y1, x2, y2, x, y).unwrap();
    }

    fn close(&mut self) {
        self.path.close().unwrap();
    }
}
