use crate::canvas_backend::CanvasBackend;
use crate::pdf_canvas::PdfCanvas;
use crate::text_state::TextState;
use crate::{error::PdfCanvasError, text_renderer::TextRenderer};
use pdf_graphics::transform::Transform;
use read_fonts::TableProvider;
use skrifa::outline::OutlineGlyphCollection;
use skrifa::{FontRef, GlyphId, MetadataProvider};

pub(crate) struct Type1FontRenderer<'a, 'b, B: CanvasBackend> {
    canvas: &'b mut PdfCanvas<'a, B>,
    font_ref: FontRef<'b>,
    outlines: OutlineGlyphCollection<'b>,
    /// Whether the incoming values are CIDs from a composite Type0 font.
    is_cid: bool,
    /// Base transformation matrix incorporating font size, horizontal scaling,
    /// and text rise, computed once in `new()` using the font's actual UPE.
    glyph_base_transform: Transform,
    /// Font design units per em, read from the `head` table in `new()`.
    units_per_em: u16,
}

impl<'a, 'b, B: CanvasBackend> Type1FontRenderer<'a, 'b, B> {
    pub fn new(
        canvas: &'b mut PdfCanvas<'a, B>,
        font_bytes: &'b [u8],
        is_cid: bool,
    ) -> Result<Self, PdfCanvasError> {
        let font_ref = FontRef::new(font_bytes)
            .map_err(|_| PdfCanvasError::InvalidFont("unrecognized Type 1 font data".into()))?;

        let outlines = font_ref.outline_glyphs();

        let units_per_em = font_ref
            .head()
            .ok()
            .map(|h| h.units_per_em())
            .filter(|&upe| {
                (TextState::MIN_UNITS_PER_EM..=TextState::MAX_UNITS_PER_EM).contains(&upe)
            })
            .unwrap_or(TextState::DEFAULT_UNITS_PER_EM);

        let upe_inv = 1.0 / f32::from(units_per_em);

        let glyph_base_transform = canvas
            .current_state()?
            .text_state
            .glyph_base_transform(upe_inv);

        Ok(Self {
            canvas,
            font_ref,
            outlines,
            is_cid,
            glyph_base_transform,
            units_per_em,
        })
    }
}

impl<B: CanvasBackend> TextRenderer for Type1FontRenderer<'_, '_, B> {
    fn render_text(&mut self, iter: impl Iterator<Item = u16>) -> Result<(), PdfCanvasError> {
        let cff = self.font_ref.cff().map_err(|_| {
            PdfCanvasError::InvalidFont("failed to read the CFF table from the Type 1 font".into())
        })?;

        let charset = cff.charset(0).map_err(|_| {
            PdfCanvasError::InvalidFont("failed to read the Type 1 font CFF charset".into())
        })?;

        for char_code in iter {
            let state = self.canvas.current_state()?;
            let glyph_matrix_for_char = state
                .text_state
                .compose_glyph_matrix(self.glyph_base_transform, &state.transform);

            // Simple Type1 fonts resolve through glyph names in the CFF charset.
            // CID-keyed CFF fonts already arrive here as decoded CIDs.
            let gid = if self.is_cid {
                GlyphId::new(u32::from(char_code))
            } else if let Some(charset) = &charset {
                let name = state.text_state.glyph_name(char_code).unwrap_or(".notdef");

                charset
                    .iter()
                    .find_map(|(i, s)| {
                        let is_match = s
                            .resolve_standard()
                            .map(|st| st == name.as_bytes())
                            .unwrap_or(false);
                        if is_match { Some(i) } else { None }
                    })
                    .unwrap_or(GlyphId::NOTDEF)
            } else {
                GlyphId::new(u32::from(char_code))
            };

            if let Some(outline_glyph) = self.outlines.get(gid) {
                self.canvas
                    .draw_outline_glyph(&outline_glyph, &glyph_matrix_for_char)?;
            }

            self.canvas
                .current_state_mut()?
                .text_state
                .advance_horizontal_glyph(char_code, &self.font_ref, gid, self.units_per_em)?;
        }
        Ok(())
    }
}
