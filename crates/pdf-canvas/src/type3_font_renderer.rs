use pdf_content_stream::{
    pdf_operator::PdfOperatorVariant, pdf_operator_backend::PdfOperatorBackend,
};
use pdf_font::type3_font::Type3Font;
use pdf_graphics::transform::Transform;
use thiserror::Error;

use crate::{canvas::Canvas, text_renderer::TextRenderer};

/// Defines errors that can occur during Type 3 font rendering.
#[derive(Debug, Error)]
pub enum Type3FontRendererError {
    #[error("Invalid /FontMatrix. Expected an array of 6 numbers.")]
    InvalidFontMatrix,
    #[error("Error processing character procedure: {err}")]
    CharProcError { err: String },
    #[error("No character map found for font '{0}'")]
    NoCharacterMapForFont(String),
}

/// A renderer for Type 3 fonts, which defines glyphs using PDF content streams.
pub(crate) struct Type3FontRenderer<'a, T: PdfOperatorBackend + Canvas> {
    /// The canvas backend where glyphs are drawn.
    canvas: &'a mut T,
    /// The font matrix from the Type 3 font dictionary, mapping glyph space to text space.
    font_matrix: Transform,
    /// A matrix encoding font size, horizontal scaling, and text rise.
    font_size_matrix: Transform,
    /// The Current Transformation Matrix (CTM) at the time of rendering.
    current_transform: Transform,
    /// The current text matrix (Tm), which positions the text.
    text_matrix: Transform,
    /// The Type 3 font definition, containing glyph content streams.
    type3_font: &'a Type3Font,
    /// The font size.
    font_size: f32,
}

impl<'a, T: PdfOperatorBackend + Canvas> Type3FontRenderer<'a, T> {
    pub(crate) fn new(
        canvas: &'a mut T,
        font_size: f32,
        horizontal_scaling: f32,
        text_rise: f32,
        current_transform: Transform,
        text_matrix: Transform,
        type3_font: &'a Type3Font,
    ) -> Result<Self, Type3FontRendererError> {
        let font_matrix = if let [a, b, c, d, e, f] = type3_font.font_matrix.as_slice() {
            Transform::from_row(*a, *b, *c, *d, *e, *f)
        } else {
            return Err(Type3FontRendererError::InvalidFontMatrix);
        };

        // For Type 3 fonts, each glyph's transformation is computed as CTM * Tm * S * FontMatrix
        // S encodes font size (Tfs), horizontal scaling (Th), and text rise (Ts)
        // S = [Tfs * Th 0 0 Tfs 0 Ts] in matrix notation
        // We precompute this combined transformation; concat applies each matrix in sequence (pre-multiplied)
        let th_factor = horizontal_scaling / 100.0;
        let font_size_matrix = Transform::from_row(
            font_size * th_factor, // sx
            0.0,                   // ky
            0.0,                   // kx
            font_size,             // sy
            0.0,                   // tx
            text_rise,             // ty
        );

        Ok(Self {
            canvas,
            font_matrix,
            font_size_matrix,
            current_transform,
            text_matrix,
            type3_font,
            font_size,
        })
    }
}

impl<T: PdfOperatorBackend + Canvas> TextRenderer for Type3FontRenderer<'_, T> {
    fn render_text(&mut self, text: &[u8]) -> Result<(), crate::error::PdfCanvasError> {
        // 1. Iterate through each character code in the input text.
        let iter = text.iter().copied();
        for char_code_byte in iter {
            let mut text_rendering_matrix = self.font_matrix;
            // Multiply by the font size, horizontal scaling, and rise matrix (S).
            text_rendering_matrix.concat(&self.font_size_matrix);
            // Multiply by the current text matrix (Tm).
            text_rendering_matrix.concat(&self.text_matrix);
            // Multiply by the current transformation matrix (CTM).
            text_rendering_matrix.concat(&self.current_transform);

            // 2. Map character code to glyph name using the font's encoding.
            let glyph_name = self
                .type3_font
                .encoding
                .as_ref()
                .and_then(|enc| enc.differences.get(&char_code_byte));

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
                // The glyph width is given in glyph space. Scale it up by
                // the font size and a conventional 1000-unit glyph space grid to
                // calculate the final horizontal displacement in text space.
                let advance = width * self.font_size / 1000.0;
                self.text_matrix.translate(advance, 0.0);
            }
        }

        Ok(())
    }
}
