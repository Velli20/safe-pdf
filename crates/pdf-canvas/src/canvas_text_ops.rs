use crate::error::PdfCanvasError;
use crate::pdf_canvas::PdfCanvas;
use crate::text_renderer::TextRenderer;
use crate::truetype_font_renderer::TrueTypeFontRenderer;
use crate::type1_font_renderer::Type1FontRenderer;
use crate::type3_font_renderer::Type3FontRenderer;
use num_traits::FromPrimitive;
use pdf_content_stream::TextElement;
use pdf_content_stream::pdf_operator_backend::{
    TextObjectOps, TextPositioningOps, TextShowingOps, TextStateOps,
};
use pdf_font::font::Font;
use pdf_font::type0_font::CidFontSubType;
use pdf_graphics::TextRenderingMode;
use pdf_graphics::transform::Transform;

impl<T: std::error::Error> TextPositioningOps for PdfCanvas<'_, T> {
    fn move_text_position(&mut self, tx: f32, ty: f32) -> Result<(), Self::ErrorType> {
        let mat = Transform::from_translate(tx, ty);
        // PDF 1.7 (Tj and text positioning): Td updates Tlm = Tlm * T(tx, ty), then Tm = Tlm.
        // Use post-multiplication to move in text space coordinates.
        self.current_state_mut()?
            .text_state
            .line_matrix
            .post_concat(&mat);
        let lm = self.current_state()?.text_state.line_matrix;
        self.current_state_mut()?.text_state.matrix = lm;
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
        // Tm operator sets both Tm and Tlm to the same matrix.
        self.current_state_mut()?.text_state.line_matrix = *transform;
        self.current_state_mut()?.text_state.matrix = *transform;
        Ok(())
    }

    fn move_to_start_of_next_line(&mut self) -> Result<(), Self::ErrorType> {
        // T*: Move to start of next line using current leading: Tlm = Tlm * T(0, -Tl); Tm = Tlm.
        let leading = self.current_state()?.text_state.leading;
        let mat = Transform::from_translate(0.0, -leading);
        self.current_state_mut()?
            .text_state
            .line_matrix
            .post_concat(&mat);
        let lm = self.current_state()?.text_state.line_matrix;
        self.current_state_mut()?.text_state.matrix = lm;
        Ok(())
    }
}

impl<T: std::error::Error> TextObjectOps for PdfCanvas<'_, T> {
    fn begin_text_object(&mut self) -> Result<(), Self::ErrorType> {
        self.current_state_mut()?.text_state.matrix = Transform::identity();
        self.current_state_mut()?.text_state.line_matrix = Transform::identity();
        Ok(())
    }

    fn end_text_object(&mut self) -> Result<(), Self::ErrorType> {
        Ok(())
    }
}

impl<T: std::error::Error> TextStateOps for PdfCanvas<'_, T> {
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

    fn set_font_and_size(&mut self, font_name: &str, size: f32) -> Result<(), Self::ErrorType> {
        self.current_state_mut()?.text_state.font_size = size;

        if let Some(resources) = self.current_state()?.resources
            && let Some(font) = resources.fonts.get(font_name)
        {
            self.current_state_mut()?.text_state.font = Some(font);
            return Ok(());
        }

        Err(PdfCanvasError::FontNotFound(font_name.to_string()))
    }

    fn set_text_rendering_mode(&mut self, mode: TextRenderingMode) -> Result<(), Self::ErrorType> {
        self.current_state_mut()?.rendering_mode = Some(mode);
        Ok(())
    }

    fn set_text_rise(&mut self, rise: f32) -> Result<(), Self::ErrorType> {
        self.current_state_mut()?.text_state.rise = rise;
        Ok(())
    }
}

/// Create an iterator over big-endian CID values from a byte slice.
fn to_cid_char_iter(text: &[u8]) -> impl Iterator<Item = u16> + '_ {
    text.chunks_exact(2).map(|pair| {
        let mut iter = pair.iter().copied();
        let first_byte = iter.next().unwrap_or(0);
        let second_byte = iter.next().unwrap_or(0);
        u16::from_be_bytes([first_byte, second_byte])
    })
}

/// Create an iterator over single-byte character codes as `u16`.
fn to_char_iter(text: &[u8]) -> impl Iterator<Item = u16> + '_ {
    text.iter().copied().map(|b| u16::from_u8(b).unwrap_or(0))
}

impl<T: std::error::Error> TextShowingOps for PdfCanvas<'_, T> {
    fn show_text(&mut self, text: &[u8]) -> Result<(), Self::ErrorType> {
        let current_font = self
            .current_state()?
            .text_state
            .font
            .ok_or(PdfCanvasError::NoCurrentFont)?;

        match current_font {
            Font::Type3(type3_font) => {
                let iter = to_char_iter(text);
                let mut renderer = Type3FontRenderer::new(self, type3_font)?;
                renderer.render_text(iter)
            }
            Font::Type1(type1_font) => {
                let program = type1_font.font_file.as_slice();
                let iter = to_char_iter(text);

                let mut renderer = Type1FontRenderer::new(self, program)?;
                renderer.render_text(iter)
            }
            Font::TrueType(font) => {
                let iter = to_char_iter(text);

                let mut renderer = TrueTypeFontRenderer::new(self, &font.font_file, false)?;
                renderer.render_text(iter)
            }
            Font::Type0(font) => {
                let iter = to_cid_char_iter(text);

                match font.subtype {
                    CidFontSubType::Type0 => {
                        let program = font.font_file.as_slice();
                        let mut renderer = Type1FontRenderer::new(self, program)?;
                        renderer.render_text(iter)
                    }
                    CidFontSubType::Type2 => {
                        let mut renderer = TrueTypeFontRenderer::new(self, &font.font_file, true)?;
                        renderer.render_text(iter)
                    }
                }
            }
        }
    }

    fn show_text_with_glyph_positioning(
        &mut self,
        elements: &[pdf_content_stream::TextElement],
    ) -> Result<(), Self::ErrorType> {
        for element in elements {
            match element {
                TextElement::Text { value } => {
                    self.show_text(value.as_bytes())?;
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
        Err(PdfCanvasError::NotImplemented(format!(
            "move_to_next_line_and_show_text ' (text_len={})",
            text.len()
        )))
    }

    fn set_spacing_and_show_text(
        &mut self,
        word_spacing: f32,
        char_spacing: f32,
        text: &[u8],
    ) -> Result<(), Self::ErrorType> {
        Err(PdfCanvasError::NotImplemented(format!(
            "set_spacing_and_show_text \" : word_spacing={}, char_spacing={}, text_len={}",
            word_spacing,
            char_spacing,
            text.len()
        )))
    }
}
