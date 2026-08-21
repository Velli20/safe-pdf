use crate::canvas_backend::CanvasBackend;
use crate::error::PdfCanvasError;
use crate::pdf_canvas::PdfCanvas;
use crate::text_renderer::TextRenderer;
use crate::truetype_font_renderer::TrueTypeFontRenderer;
use crate::type1_font_renderer::Type1FontRenderer;
use crate::type3_font_renderer::Type3FontRenderer;
use num_traits::FromPrimitive;
use pdf_content_stream_operators::TextElement;
use pdf_content_stream_operators::pdf_operator_backend::{
    TextObjectOps, TextPositioningOps, TextShowingOps, TextStateOps,
};
use pdf_font::flags::FontFlags;
use pdf_font::font::Font;
use pdf_font::type0_font::Type0FontProgramFormat;
use pdf_font::type1_font::Type1FontProgramFormat;
use pdf_graphics::TextRenderingMode;
use pdf_graphics::transform::Transform;

impl<B: CanvasBackend> TextPositioningOps for PdfCanvas<'_, B> {
    type ErrorType = PdfCanvasError;
    fn move_text_position(&mut self, tx: f32, ty: f32) -> Result<(), Self::ErrorType> {
        let mat = Transform::from_translate(tx, ty);
        let state = self.current_state_mut()?;
        // PDF 1.7 (Tj and text positioning): Td updates Tlm = Tlm * T(tx, ty), then Tm = Tlm.
        // Use post-multiplication to move in text space coordinates.
        state.text_state.line_matrix.post_concat(&mat);
        let lm = state.text_state.line_matrix;
        state.text_state.matrix = lm;
        Ok(())
    }

    fn move_text_position_and_set_leading(
        &mut self,
        tx: f32,
        ty: f32,
    ) -> Result<(), Self::ErrorType> {
        // TD: Set leading to -ty, then perform Td(tx, ty)
        let neg_ty = -ty;
        self.current_state_mut()?.text_state.leading = neg_ty;
        self.move_text_position(tx, ty)
    }

    fn set_text_matrix(&mut self, transform: &Transform) -> Result<(), Self::ErrorType> {
        let state = self.current_state_mut()?;
        // Tm operator sets both Tm and Tlm to the same matrix.
        state.text_state.line_matrix = *transform;
        state.text_state.matrix = *transform;
        Ok(())
    }

    fn move_to_start_of_next_line(&mut self) -> Result<(), Self::ErrorType> {
        // T*: Move to start of next line using current leading: Tlm = Tlm * T(0, -Tl); Tm = Tlm.
        let leading = self.current_state()?.text_state.leading;
        let state = self.current_state_mut()?;

        let mat = Transform::from_translate(0.0, -leading);
        state.text_state.line_matrix.post_concat(&mat);
        let lm = state.text_state.line_matrix;
        state.text_state.matrix = lm;
        Ok(())
    }
}

impl<B: CanvasBackend> TextObjectOps for PdfCanvas<'_, B> {
    type ErrorType = PdfCanvasError;
    fn begin_text_object(&mut self) -> Result<(), Self::ErrorType> {
        let state = self.current_state_mut()?;
        state.text_state.matrix = Transform::identity();
        state.text_state.line_matrix = Transform::identity();
        // Clear any stale clip accumulator (guards against a missing ET from a prior object).
        state.pending_text_clip = None;
        Ok(())
    }

    fn end_text_object(&mut self) -> Result<(), Self::ErrorType> {
        // Apply any glyph outlines accumulated for clip-mode text rendering (modes 4–7).
        // Per ISO 32000 §9.3.6, the clip path is set at the end of the text object.
        if let Some(clip_path) = self.current_state_mut()?.pending_text_clip.take() {
            self.canvas
                .set_clip_region(&clip_path, pdf_graphics::PathFillType::Winding)?;
            self.current_state_mut()?.clip_path = Some(clip_path);
        }
        Ok(())
    }
}

impl<B: CanvasBackend> TextStateOps for PdfCanvas<'_, B> {
    type ErrorType = PdfCanvasError;
    fn set_character_spacing(&mut self, spacing: f32) -> Result<(), Self::ErrorType> {
        self.current_state_mut()?.text_state.character_spacing = spacing;
        Ok(())
    }

    fn set_word_spacing(&mut self, spacing: f32) -> Result<(), Self::ErrorType> {
        self.current_state_mut()?.text_state.word_spacing = spacing;
        Ok(())
    }

    fn set_horizontal_text_scaling(&mut self, scale_percent: f32) -> Result<(), Self::ErrorType> {
        self.current_state_mut()?.text_state.horizontal_scaling = scale_percent / 100.0;
        Ok(())
    }

    fn set_text_leading(&mut self, leading: f32) -> Result<(), Self::ErrorType> {
        // TL sets the text leading parameter
        self.current_state_mut()?.text_state.leading = leading;
        Ok(())
    }

    fn set_font_and_size(&mut self, font_name: &[u8], size: f32) -> Result<(), Self::ErrorType> {
        let state = self.current_state_mut()?;
        state.text_state.font_size = size;

        if let Some(resources) = state.resources
            && let Some((font, nested_resources)) = resources.font(font_name)
        {
            state.text_state.font = Some(font);
            state.text_state.resources = nested_resources;
            return Ok(());
        }

        Err(PdfCanvasError::FontNotFound(
            String::from_utf8_lossy(font_name).into_owned(),
        ))
    }

    fn set_text_rendering_mode(&mut self, mode: TextRenderingMode) -> Result<(), Self::ErrorType> {
        self.current_state_mut()?.rendering_mode = mode;
        Ok(())
    }

    fn set_text_rise(&mut self, rise: f32) -> Result<(), Self::ErrorType> {
        self.current_state_mut()?.text_state.rise = rise;
        Ok(())
    }
}

/// Create an iterator over single-byte character codes as `u16`.
fn to_char_iter(text: &[u8]) -> impl Iterator<Item = u16> + '_ {
    text.iter().copied().map(|b| u16::from_u8(b).unwrap_or(0))
}

impl<B: CanvasBackend> TextShowingOps for PdfCanvas<'_, B> {
    type ErrorType = PdfCanvasError;
    fn show_text(&mut self, text: &[u8]) -> Result<(), Self::ErrorType> {
        let current_font = self
            .current_state()?
            .text_state
            .font
            .ok_or(PdfCanvasError::CurrentFontRequired)?;

        match current_font {
            Font::Type3(type3_font) => {
                let iter = to_char_iter(text);
                let mut renderer = Type3FontRenderer::new(self, type3_font)?;
                renderer.render_text(iter)
            }
            Font::Type1(type1_font) => {
                let program = type1_font.font_file.as_ref();
                let iter = to_char_iter(text);

                let mut renderer =
                    Type1FontRenderer::new(self, program, type1_font.program_format, false)?;
                renderer.render_text(iter)
            }
            Font::TrueType(font) => {
                let iter = to_char_iter(text);
                let is_symbolic = font.flags.contains(FontFlags::SYMBOLIC);
                let mut renderer =
                    TrueTypeFontRenderer::new(self, &font.font_file, false, is_symbolic, false)?;
                renderer.render_text(iter)
            }
            Font::Type0(font) => {
                let decoded_cids = font.decode_bytes_to_cids(text);
                let iter = decoded_cids.into_iter();

                match font.program_format {
                    Type0FontProgramFormat::OpenTypeCff => {
                        let program = font.font_file.as_ref();
                        let program_format = font
                            .type1_program_format
                            .unwrap_or(Type1FontProgramFormat::OpenTypeCff);
                        let mut renderer =
                            Type1FontRenderer::new(self, program, program_format, true)?;
                        renderer.render_text(iter)
                    }
                    Type0FontProgramFormat::TrueType { cid_to_unicode } => {
                        // CID TrueType fonts use glyph IDs directly; symbolic flag is irrelevant.
                        let mut renderer = TrueTypeFontRenderer::new(
                            self,
                            &font.font_file,
                            true,
                            false,
                            cid_to_unicode,
                        )?;
                        renderer.render_text(iter)
                    }
                }
            }
        }
    }

    fn show_text_with_glyph_positioning(
        &mut self,
        elements: &[pdf_content_stream_operators::TextElement],
    ) -> Result<(), Self::ErrorType> {
        for element in elements {
            match element {
                TextElement::Text { value } => {
                    self.show_text(value)?;
                }
                TextElement::Adjustment { amount } => {
                    // TJ adjustment: Tm = Tm * T( -amount/1000 * Tfs * Th, 0 )
                    let amount = (*amount) / 1000.0;
                    let state = self.current_state_mut()?;
                    let tx =
                        -amount * state.text_state.font_size * state.text_state.horizontal_scaling;
                    state.text_state.matrix.post_translate(tx, 0.0);
                }
                TextElement::HexString { value } => {
                    self.show_text(value)?;
                }
            }
        }
        Ok(())
    }

    fn move_to_next_line_and_show_text(&mut self, text: &[u8]) -> Result<(), Self::ErrorType> {
        self.move_to_start_of_next_line()?;
        self.show_text(text)
    }

    fn set_spacing_and_show_text(
        &mut self,
        word_spacing: f32,
        char_spacing: f32,
        text: &[u8],
    ) -> Result<(), Self::ErrorType> {
        self.set_word_spacing(word_spacing)?;
        self.set_character_spacing(char_spacing)?;
        self.move_to_next_line_and_show_text(text)
    }
}
