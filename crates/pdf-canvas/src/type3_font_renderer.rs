use pdf_content_stream_operators::variants::PdfOperatorVariant;
use pdf_font::type3_font::Type3Font;
use pdf_graphics::transform::Transform;

use crate::{
    canvas_backend::CanvasBackend, error::PdfCanvasError, pdf_canvas::PdfCanvas,
    text_renderer::TextRenderer,
};

/// A renderer for Type 3 fonts, which defines glyphs using PDF content streams.
pub(crate) struct Type3FontRenderer<'a, 'b, B: CanvasBackend> {
    /// A mutable reference to the `PdfCanvas` where glyphs are drawn.
    canvas: &'b mut PdfCanvas<'a, B>,
    /// A matrix encoding font size, horizontal scaling, and text rise.
    font_size_matrix: Transform,
    /// The Type 3 font definition, containing glyph content streams.
    type3_font: &'a Type3Font,
}

impl<'a, 'b, B: CanvasBackend> Type3FontRenderer<'a, 'b, B> {
    pub(crate) fn new(
        canvas: &'b mut PdfCanvas<'a, B>,
        type3_font: &'a Type3Font,
    ) -> Result<Self, PdfCanvasError> {
        // For Type 3 fonts, each glyph's transformation is computed as CTM * Tm * S * FontMatrix
        // S encodes font size (Tfs), horizontal scaling (Th), and text rise (Ts):
        // S = [Tfs*Th 0 0 Tfs 0 Ts], which is exactly glyph_base_transform(1.0).
        let font_size_matrix = canvas.current_state()?.text_state.glyph_base_transform(1.0);

        Ok(Self {
            canvas,
            font_size_matrix,
            type3_font,
        })
    }
}

impl<B: CanvasBackend> TextRenderer for Type3FontRenderer<'_, '_, B> {
    fn render_text(
        &mut self,
        iter: impl Iterator<Item = u16>,
    ) -> Result<(), crate::error::PdfCanvasError> {
        for char_code_byte in iter {
            let state = self.canvas.current_state()?;

            // Compute a relative glyph matrix (Tm × S × FontMatrix) without the CTM.
            // render_content_stream will post-concatenate this onto the current CTM,
            // yielding the same absolute matrix: CTM × Tm × S × FontMatrix.
            let glyph_matrix = {
                let mut base = self.type3_font.font_matrix;
                base.concat(&self.font_size_matrix);
                base.concat(&state.text_state.matrix);
                base
            };

            let glyph_name = state.text_state.glyph_name(char_code_byte);
            let Some(glyph_name) = glyph_name else {
                continue;
            };

            let Some(char_procs) = self.type3_font.char_procs.get(glyph_name) else {
                continue;
            };

            // Type 3 fonts may carry their own resource dictionary for glyph procedures.
            let type3_resources = state.text_state.resources;

            // Intercept d0/d1 operators via the filter to capture glyph width.
            // These operators are no-ops on the backend; the filter skips them
            // while recording the width for text cursor advancement.
            //
            // Note: For clip rendering modes (TextRenderingMode 4–7), Type 3 glyph outlines
            // should also be accumulated into the text clip path (ISO 32000 §9.3.6). This
            // requires intercepting the paths painted by the glyph's content stream procedure,
            // which is not yet implemented. Clip modes for Type 3 fonts are therefore a no-op
            // for the clip accumulation step; only the paint part (fill/stroke) takes effect
            // because the glyph's procedure calls standard painting operators directly.
            let mut glyph_width = None;
            let mut filter = |op: &PdfOperatorVariant| -> bool {
                match op {
                    PdfOperatorVariant::SetCharWidthAndBoundingBox(d1) => {
                        glyph_width = Some(d1.wx);
                        true
                    }
                    PdfOperatorVariant::SetCharWidth(d0) => {
                        glyph_width = Some(d0.wx);
                        true
                    }
                    _ => false,
                }
            };

            self.canvas.render_content_stream(
                char_procs,
                Some(glyph_matrix),
                None,
                type3_resources,
                Some(&mut filter),
            )?;

            // Advance the text matrix (Tm) to position the next glyph.
            if let Some(width) = glyph_width {
                let text_state = &mut self.canvas.current_state_mut()?.text_state;

                // Compute displacement vector in text space for (width, 0) in glyph space
                let (x1, y1) = self.type3_font.font_matrix.transform_point(width, 0.0);
                let (x0, y0) = self.type3_font.font_matrix.transform_point(0.0, 0.0);
                let glyph_width_x = (x1 - x0) * text_state.font_size;
                let glyph_width_y = (y1 - y0) * text_state.font_size;

                text_state.advance_text_cursor(char_code_byte, glyph_width_x, glyph_width_y);
            }
        }

        Ok(())
    }
}
