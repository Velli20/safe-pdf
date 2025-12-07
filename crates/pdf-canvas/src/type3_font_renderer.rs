use pdf_content_stream::pdf_operator::PdfOperatorVariant;
use pdf_font::type3_font::Type3Font;
use pdf_graphics::transform::Transform;
use thiserror::Error;

use crate::{
    error::PdfCanvasError, pdf_canvas::PdfCanvas, text_renderer::TextRenderer,
    text_state::TextState,
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
pub(crate) struct Type3FontRenderer<'a, 'b, T: std::error::Error> {
    /// A mutable reference to the `PdfCanvas` where glyphs are drawn.
    canvas: &'b mut PdfCanvas<'a, T>,
    /// The font matrix from the Type 3 font dictionary, mapping glyph space to text space.
    font_matrix: Transform,
    /// A matrix encoding font size, horizontal scaling, and text rise.
    font_size_matrix: Transform,
    /// The Type 3 font definition, containing glyph content streams.
    type3_font: &'a Type3Font,
}

impl<'a, 'b, T: std::error::Error> Type3FontRenderer<'a, 'b, T> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        canvas: &'b mut PdfCanvas<'a, T>,
        type3_font: &'a Type3Font,
    ) -> Result<Self, PdfCanvasError> {
        let font_matrix = if let [a, b, c, d, e, f] = type3_font.font_matrix.as_slice() {
            Transform::from_row(*a, *b, *c, *d, *e, *f)
        } else {
            return Err(Type3FontRendererError::InvalidFontMatrix.into());
        };

        // Extract text state parameters for rendering.
        let TextState {
            horizontal_scaling,
            font_size,
            rise,
            ..
        } = canvas.current_state()?.text_state.clone();

        // For Type 3 fonts, each glyph's transformation is computed as CTM * Tm * S * FontMatrix
        // S encodes font size (Tfs), horizontal scaling (Th), and text rise (Ts)
        // S = [Tfs * Th 0 0 Tfs 0 Ts] in matrix notation
        // We precompute this combined transformation; concat applies each matrix in sequence (pre-multiplied)
        let font_size_matrix = Transform::from_row(
            font_size * horizontal_scaling, // sx
            0.0,                            // ky
            0.0,                            // kx
            font_size,                      // sy
            0.0,                            // tx
            rise,                           // ty
        );

        Ok(Self {
            canvas,
            font_matrix,
            font_size_matrix,
            type3_font,
        })
    }
}

impl<T: std::error::Error> TextRenderer for Type3FontRenderer<'_, '_, T> {
    fn render_text(
        &mut self,
        iter: &mut dyn Iterator<Item = u16>,
    ) -> Result<(), crate::error::PdfCanvasError> {
        // 1. Iterate through each character code in the input text.
        for char_code_byte in iter {
            let state = self.canvas.current_state()?;
            let mut text_rendering_matrix = self.font_matrix;
            // Multiply by the font size, horizontal scaling, and rise matrix (S).
            text_rendering_matrix.concat(&self.font_size_matrix);
            // Multiply by the current text matrix (Tm).
            text_rendering_matrix.concat(&state.text_state.matrix);
            // Multiply by the current transformation matrix (CTM).
            text_rendering_matrix.concat(&state.transform);

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

            let mut glyph_width = None;

            // 5. Set the transformation matrix for the glyph and execute its content stream.
            // The CTM is temporarily replaced with the computed text rendering matrix.
            self.canvas.set_matrix(text_rendering_matrix)?;

            for op in char_procs {
                // Check if this the `d1` operator. The `d1` operator is only used within the
                // content stream of a Type 3 font's character procedure. It sets the width
                // and bounding box of the character being defined.
                // The backend is responsible for storing the width (`wx`, `wy`)
                // so it can be used to advance the text matrix after the glyph is painted.
                if let PdfOperatorVariant::SetCharWidthAndBoundingBox(op) = op {
                    glyph_width = Some(op.wx);
                } else {
                    op.call(self.canvas)
                        .map_err(|err| Type3FontRendererError::CharProcError {
                            err: format!("Error calling operator: {:?}", err),
                        })?;
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
                let (wx_ts, wy_ts) = (x1 - x0, y1 - y0);

                let base_adv_x = wx_ts * text_state.font_size;
                let advance_y = wy_ts * text_state.font_size;

                let word_spacing_for_char = if char_code_byte == 32 {
                    text_state.word_spacing
                } else {
                    0.0
                };
                let advance_x = (base_adv_x + text_state.character_spacing + word_spacing_for_char)
                    * text_state.horizontal_scaling;

                text_state.matrix.post_translate(advance_x, advance_y);
            }
        }

        Ok(())
    }
}
