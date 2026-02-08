use crate::pdf_canvas::PdfCanvas;
use crate::pdf_path_pen::PdfPathPen;
use crate::text_state::TextState;
use crate::{error::PdfCanvasError, text_renderer::TextRenderer};
use pdf_graphics::transform::Transform;
use pdf_graphics::{PaintMode, PathFillType};
use read_fonts::TableProvider;
use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, OutlineGlyphCollection};
use skrifa::{FontRef, GlyphId, MetadataProvider};

pub(crate) struct Type1FontRenderer<'a, 'b, T: std::error::Error> {
    canvas: &'b mut PdfCanvas<'a, T>,
    font_ref: FontRef<'b>,
    outlines: OutlineGlyphCollection<'b>,
}

impl<'a, 'b, T: std::error::Error> Type1FontRenderer<'a, 'b, T> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        canvas: &'b mut PdfCanvas<'a, T>,
        font_bytes: &'b [u8],
    ) -> Result<Self, PdfCanvasError> {
        let font_ref = FontRef::new(font_bytes)
            .map_err(|_| PdfCanvasError::InvalidFont("invalid font data"))?;

        let outlines = font_ref.outline_glyphs();

        Ok(Self {
            canvas,
            font_ref,
            outlines,
        })
    }
}

impl<T: std::error::Error> TextRenderer for Type1FontRenderer<'_, '_, T> {
    fn render_text(&mut self, iter: impl Iterator<Item = u16>) -> Result<(), PdfCanvasError> {
        let TextState {
            horizontal_scaling,
            font_size,
            rise,
            ..
        } = self.canvas.current_state()?.text_state.clone();

        let scale = font_size * 0.001;
        let m_params = Transform::from_row(scale * horizontal_scaling, 0.0, 0.0, scale, 0.0, rise);

        let cff = self
            .font_ref
            .cff()
            .map_err(|_| PdfCanvasError::InvalidFont("failed to read CFF table from Type1 font"))?;

        let charset = cff
            .charset(0)
            .map_err(|_| PdfCanvasError::InvalidFont("failed to read CFF charset"))?;

        for char_code in iter {
            let mut glyph_matrix_for_char = m_params;
            let state = &self.canvas.current_state()?;
            glyph_matrix_for_char.concat(&state.text_state.matrix);
            glyph_matrix_for_char.concat(&state.transform);

            // Resolve glyph id from CFF charset.
            let gid = if let Some(charset) = &charset {
                let name = state.text_state.glyph_name(char_code).unwrap_or(".notdef");

                charset
                    .iter()
                    .find_map(|(i, s)| {
                        let is_match = s.standard_string().map(|st| st == name).unwrap_or(false);
                        if is_match { Some(i) } else { None }
                    })
                    .unwrap_or(GlyphId::NOTDEF)
            } else {
                GlyphId::new(u32::from(char_code))
            };

            if let Some(outline_glyph) = self.outlines.get(gid) {
                let mut pen = PdfPathPen::default();
                // Draw unhinted at requested font size.
                let size = Size::new(1000.0);
                let settings = DrawSettings::from((size, LocationRef::default()));
                if outline_glyph.draw(settings, &mut pen).is_ok() {
                    pen.path.transform(&glyph_matrix_for_char);
                    self.canvas
                        .draw_path(&pen.path, PaintMode::Fill, PathFillType::Winding)?;
                } else {
                    // println!(
                    //     "Failed to draw outline for char code {}",
                    //     char::from(char_code)
                    // );
                }
            } else {
                println!("No outline for gid {:?} ", gid,);
            }

            // Advance text matrix.
            let text_state = &mut self.canvas.current_state_mut()?.text_state;
            let glyph_width_x = text_state.glyph_width(char_code) / 1000.0 * text_state.font_size;
            text_state.advance_text_cursor(char_code, glyph_width_x, 0.0);
        }
        Ok(())
    }
}
