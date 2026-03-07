use crate::canvas_backend::CanvasBackend;
use crate::pdf_canvas::PdfCanvas;
use crate::pdf_path_pen::draw_outline_glyph;
use crate::{error::PdfCanvasError, text_renderer::TextRenderer};
use read_fonts::TableProvider;
use skrifa::instance::Size;
use skrifa::outline::OutlineGlyphCollection;
use skrifa::{FontRef, GlyphId, MetadataProvider};

pub(crate) struct Type1FontRenderer<'a, 'b, B: CanvasBackend> {
    canvas: &'b mut PdfCanvas<'a, B>,
    font_ref: FontRef<'b>,
    outlines: OutlineGlyphCollection<'b>,
}

impl<'a, 'b, B: CanvasBackend> Type1FontRenderer<'a, 'b, B> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        canvas: &'b mut PdfCanvas<'a, B>,
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

impl<B: CanvasBackend> TextRenderer for Type1FontRenderer<'_, '_, B> {
    fn render_text(&mut self, iter: impl Iterator<Item = u16>) -> Result<(), PdfCanvasError> {
        let m_params = self
            .canvas
            .current_state()?
            .text_state
            .glyph_base_transform(0.001);

        let cff = self
            .font_ref
            .cff()
            .map_err(|_| PdfCanvasError::InvalidFont("failed to read CFF table from Type1 font"))?;

        let charset = cff
            .charset(0)
            .map_err(|_| PdfCanvasError::InvalidFont("failed to read CFF charset"))?;

        for char_code in iter {
            let state = self.canvas.current_state()?;
            let glyph_matrix_for_char = state
                .text_state
                .compose_glyph_matrix(m_params, &state.transform);

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
                draw_outline_glyph(
                    self.canvas,
                    &outline_glyph,
                    Size::new(1000.0),
                    &glyph_matrix_for_char,
                )?;
            }

            self.canvas
                .current_state_mut()?
                .text_state
                .advance_horizontal_glyph(char_code);
        }
        Ok(())
    }
}
