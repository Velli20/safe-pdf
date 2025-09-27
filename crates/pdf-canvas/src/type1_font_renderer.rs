// Type1 font renderer for pdf-canvas
// This is a stub for the Type1FontRenderer. Actual glyph rasterization is not implemented yet.

use crate::{canvas::Canvas, error::PdfCanvasError, text_renderer::TextRenderer};
use pdf_content_stream::pdf_operator_backend::PdfOperatorBackend;
use pdf_font::cff::char_string_operator::CharStringStack;
use pdf_font::cff::reader::{CffFontReader, Charset};
use pdf_font::type1_font::Type1Font;
use pdf_graphics::PathFillType;
use pdf_graphics::pdf_path::PdfPath;
use pdf_graphics::transform::Transform;
use pdf_object::ObjectVariant;

pub(crate) struct Type1FontRenderer<'a, T: PdfOperatorBackend + Canvas> {
    /// The canvas backend where glyphs are drawn.
    canvas: &'a mut T,
    font: &'a Type1Font,
    /// The current text matrix (Tm), which positions the text.
    text_matrix: Transform,
    /// The Current Transformation Matrix (CTM) at the time of rendering.
    current_transform: Transform,
    /// The font size in user space units.
    font_size: f32,
    /// The text rise (Ts), a vertical offset from the baseline.
    rise: f32,
    /// The horizontal scaling factor for glyphs, as a percentage [0-100].
    horizontal_scaling: f32,
    /// The spacing to add between words, applied to space characters.
    word_spacing: f32,
    /// The spacing to add between individual characters.
    char_spacing: f32,
}

impl<'a, T: PdfOperatorBackend + Canvas> Type1FontRenderer<'a, T> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        canvas: &'a mut T,
        font: &'a Type1Font,
        font_size: f32,
        horizontal_scaling: f32,
        text_matrix: Transform,
        current_transform: Transform,
        rise: f32,
    ) -> Self {
        Type1FontRenderer {
            canvas,
            font,
            text_matrix,
            current_transform,
            font_size,
            rise,
            horizontal_scaling,
            word_spacing: 0.0,
            char_spacing: 0.0,
        }
    }

    pub fn with_spacing(mut self, word_spacing: f32, char_spacing: f32) -> Self {
        self.word_spacing = word_spacing;
        self.char_spacing = char_spacing;
        self
    }
}

impl<T: PdfOperatorBackend + Canvas> TextRenderer for Type1FontRenderer<'_, T> {
    fn render_text(&mut self, text: &[u8]) -> Result<(), PdfCanvasError> {
        let Some(fd) = self.font.font_descriptor.as_ref() else {
            println!(
                "Type1FontRenderer: Missing FontDescriptor for '{}'",
                self.font.base_font
            );
            return Ok(());
        };

        let ObjectVariant::Stream(s) = &fd.font_file else {
            return Ok(());
        };

        let program = CffFontReader::new(&s.data).read_font_program()?;

        // Build the text rendering transform.
        // CFF/Type 1 glyph outlines are expressed in a 1000 units-per-em coordinate system.
        // Scale by `font_size / 1000`, apply horizontal scaling (Th/100) and text rise.
        let th_factor = self.horizontal_scaling / 100.0;
        let scale = self.font_size * 0.001;
        let m_params = Transform::from_row(
            scale * th_factor, // sx with horizontal scaling
            0.0,               // ky (skew)
            0.0,               // kx (skew)
            scale,             // sy
            0.0,               // tx
            self.rise,         // ty
        );

        for u in text {
            // Compose the final transformation matrix for this glyph:
            // m_params -> text matrix -> current transformation matrix
            let mut glyph_matrix_for_char = m_params;
            glyph_matrix_for_char.concat(&self.text_matrix);
            glyph_matrix_for_char.concat(&self.current_transform);

            let char_code = *u;
            let ch = char::from(*u);
            let gid_opt = program.code_to_gid(char_code);

            if let Some(gid) = gid_opt {
                if (gid as usize) < program.char_string_operators.len() {
                    let glyph_ops = &program.char_string_operators[gid as usize];
                    let mut path = PdfPath::default();
                    let mut eval_stack = CharStringStack::default();
                    for (i, op) in glyph_ops.iter().enumerate() {
                        if let Err(e) = op.call(&mut path, &mut eval_stack) {
                            eprintln!(
                                "CharString op {} failed for gid {} in font {}: {}",
                                i, gid, self.font.base_font, e
                            );
                            break;
                        }
                    }
                    path.transform(&glyph_matrix_for_char);
                    self.canvas.fill_path(&path, PathFillType::Winding)?;
                } else {
                    println!(
                        "Type1FontRenderer: GID {} out of bounds (charstrings len {})",
                        gid,
                        program.char_string_operators.len()
                    );
                }
            } else {
                // Glyph ID not found for character code.
                // Use the missing width from the FontDescriptor to advance the text position.
                println!(
                    "Type1FontRenderer: No GID for char code {} ('{}') in font '{}'",
                    char_code, ch, self.font.base_font
                );
            }

            // Compute advance in text space and update Tm even if glyph wasn't drawn.
            let w0_units = self
                .font
                .widths
                .as_ref()
                .and_then(|m| m.get(&char_code).copied())
                .unwrap_or(0.0);
            let w0_ems = w0_units / 1000.0;
            let glyph_width_tfs_scaled = w0_ems * self.font_size;
            let word_spacing_for_char = if char_code == 32 {
                self.word_spacing
            } else {
                0.0
            };
            let advance_x =
                (glyph_width_tfs_scaled + self.char_spacing + word_spacing_for_char) * th_factor;
            self.text_matrix.translate(advance_x, 0.0);
        }

        Ok(())
    }
}
