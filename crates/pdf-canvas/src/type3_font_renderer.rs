use pdf_content_stream::pdf_operator::PdfOperatorVariant;
use pdf_font::type3_font::Type3Font;
use pdf_graphics::transform::Transform;
use thiserror::Error;

use crate::{
    canvas_backend::CanvasBackend, error::PdfCanvasError, pdf_canvas::PdfCanvas,
    text_renderer::TextRenderer,
};

/// Defines errors that can occur during Type 3 font rendering.
#[derive(Debug, Error)]
pub enum Type3FontRendererError {
    #[error("Invalid /FontMatrix. Expected an array of 6 numbers.")]
    InvalidFontMatrix,
    #[error("Error processing character procedure: {err}")]
    CharProcError { err: String },
}

/// A renderer for Type 3 fonts, which defines glyphs using PDF content streams.
pub(crate) struct Type3FontRenderer<'a, 'b, B: CanvasBackend> {
    /// A mutable reference to the `PdfCanvas` where glyphs are drawn.
    canvas: &'b mut PdfCanvas<'a, B>,
    /// The font matrix from the Type 3 font dictionary, mapping glyph space to text space.
    font_matrix: Transform,
    /// A matrix encoding font size, horizontal scaling, and text rise.
    font_size_matrix: Transform,
    /// The Type 3 font definition, containing glyph content streams.
    type3_font: &'a Type3Font,
}

impl<'a, 'b, B: CanvasBackend> Type3FontRenderer<'a, 'b, B> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        canvas: &'b mut PdfCanvas<'a, B>,
        type3_font: &'a Type3Font,
    ) -> Result<Self, PdfCanvasError> {
        let font_matrix = if let [a, b, c, d, e, f] = type3_font.font_matrix.as_slice() {
            Transform::from_row(*a, *b, *c, *d, *e, *f)
        } else {
            return Err(Type3FontRendererError::InvalidFontMatrix.into());
        };

        // For Type 3 fonts, each glyph's transformation is computed as CTM * Tm * S * FontMatrix
        // S encodes font size (Tfs), horizontal scaling (Th), and text rise (Ts):
        // S = [Tfs*Th 0 0 Tfs 0 Ts], which is exactly glyph_base_transform(1.0).
        let font_size_matrix = canvas.current_state()?.text_state.glyph_base_transform(1.0);

        Ok(Self {
            canvas,
            font_matrix,
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
        // 1. Iterate through each character code in the input text.
        for char_code_byte in iter {
            let state = self.canvas.current_state()?;
            let text_rendering_matrix = {
                let mut base = self.font_matrix;
                base.concat(&self.font_size_matrix);
                state
                    .text_state
                    .compose_glyph_matrix(base, &state.transform)
            };

            // 2. Map character code to glyph name using the font's encoding.
            let glyph_name = state.text_state.glyph_name(char_code_byte);
            let Some(glyph_name) = glyph_name else {
                continue;
            };

            // 3. Look up the glyph's content stream from the `CharProcs` map.
            let Some(char_procs) = self.type3_font.char_procs.get(glyph_name) else {
                // If the character code does not map to a glyph name via the font's encoding,
                // this character is skipped.
                continue;
            };

            // 4. Save graphics state before drawing the glyph.
            self.canvas.save()?;

            // Override resources with Type 3 font's own resources for this glyph.
            // Since save() cloned the state, this override is scoped to the glyph
            // and restore() will pop it.
            if let Some(type3_resources) = self.canvas.current_state()?.text_state.resources {
                self.canvas.current_state_mut()?.resources = Some(type3_resources);
            }

            let mut glyph_width = None;

            // 5. Set the transformation matrix for the glyph and execute its content stream.
            // The CTM is temporarily replaced with the computed text rendering matrix.
            //
            // Note: For clip rendering modes (TextRenderingMode 4–7), Type 3 glyph outlines
            // should also be accumulated into the text clip path (ISO 32000 §9.3.6). This
            // requires intercepting the paths painted by the glyph's content stream procedure,
            // which is not yet implemented. Clip modes for Type 3 fonts are therefore a no-op
            // for the clip accumulation step; only the paint part (fill/stroke) takes effect
            // because the glyph's procedure calls standard painting operators directly.
            self.canvas.set_matrix(text_rendering_matrix)?;

            for op in char_procs {
                // Check if this the `d1` operator. The `d1` operator is only used within the
                // content stream of a Type 3 font's character procedure. It sets the width
                // and bounding box of the character being defined.
                // The backend is responsible for storing the width (`wx`, `wy`)
                // so it can be used to advance the text matrix after the glyph is painted.
                if let PdfOperatorVariant::SetCharWidthAndBoundingBox(op) = op {
                    glyph_width = Some(op.wx);
                } else if let PdfOperatorVariant::SetCharWidth(op) = op {
                    glyph_width = Some(op.wx);
                } else {
                    op.call(self.canvas)?;
                }
            }

            // 6. Restore the original graphics state.
            self.canvas.restore()?;

            // 7. Advance the text matrix (Tm) to position the next glyph.
            if let Some(width) = glyph_width {
                let text_state = &mut self.canvas.current_state_mut()?.text_state;

                // Compute displacement vector in text space for (width, 0) in glyph space
                let (x1, y1) = self.font_matrix.transform_point(width, 0.0);
                let (x0, y0) = self.font_matrix.transform_point(0.0, 0.0);
                let glyph_width_x = (x1 - x0) * text_state.font_size;
                let glyph_width_y = (y1 - y0) * text_state.font_size;

                text_state.advance_text_cursor(char_code_byte, glyph_width_x, glyph_width_y);
            }
        }

        Ok(())
    }
}
