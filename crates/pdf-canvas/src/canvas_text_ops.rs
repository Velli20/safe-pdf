use crate::pdf_canvas::PdfCanvas;
use crate::text_renderer::TextRenderer;
use crate::truetype_font_renderer::TrueTypeFontRenderer;
use crate::type1_font_renderer::Type1FontRenderer;
use crate::type3_font_renderer::Type3FontRenderer;
use crate::{canvas_backend::CanvasBackend, error::PdfCanvasError};
use pdf_content_stream::pdf_operator_backend::{
    TextObjectOps, TextPositioningOps, TextShowingOps, TextStateOps,
};
use pdf_content_stream::TextElement;
use pdf_font::font::FontSubType;
use pdf_graphics::TextRenderingMode;
use pdf_graphics::transform::Transform;

impl<T: CanvasBackend> TextPositioningOps for PdfCanvas<'_, T> {
    fn move_text_position(&mut self, tx: f32, ty: f32) -> Result<(), Self::ErrorType> {
        let mat = Transform::from_translate(tx, ty);
        self.current_state_mut()?
            .text_state
            .line_matrix
            .concat(&mat);
        self.current_state_mut()?.text_state.matrix = self.current_state()?.text_state.line_matrix;
        Ok(())
    }

    fn move_text_position_and_set_leading(
        &mut self,
        tx: f32,
        ty: f32,
    ) -> Result<(), Self::ErrorType> {
        Err(PdfCanvasError::NotImplemented(format!(
            "move_text_position_and_set_leading TD: tx={}, ty={}",
            tx, ty
        )))
    }

    fn set_text_matrix(
        &mut self,
        a: f32,
        b: f32,
        c: f32,
        d: f32,
        e: f32,
        f: f32,
    ) -> Result<(), Self::ErrorType> {
        let mat = Transform::from_row(a, b, c, d, e, f);
        self.current_state_mut()?.text_state.line_matrix = mat;
        self.current_state_mut()?.text_state.matrix = mat;
        Ok(())
    }

    fn move_to_start_of_next_line(&mut self) -> Result<(), Self::ErrorType> {
        Err(PdfCanvasError::NotImplemented(
            "move_to_start_of_next_line T*".into(),
        ))
    }
}

impl<T: CanvasBackend> TextObjectOps for PdfCanvas<'_, T> {
    fn begin_text_object(&mut self) -> Result<(), Self::ErrorType> {
        self.current_state_mut()?.text_state.matrix = Transform::identity();
        self.current_state_mut()?.text_state.line_matrix = Transform::identity();
        Ok(())
    }

    fn end_text_object(&mut self) -> Result<(), Self::ErrorType> {
        Ok(())
    }
}

impl<T: CanvasBackend> TextStateOps for PdfCanvas<'_, T> {
    fn set_character_spacing(&mut self, spacing: f32) -> Result<(), Self::ErrorType> {
        self.current_state_mut()?.text_state.character_spacing = spacing;
        Ok(())
    }

    fn set_word_spacing(&mut self, spacing: f32) -> Result<(), Self::ErrorType> {
        self.current_state_mut()?.text_state.word_spacing = spacing;
        Ok(())
    }

    fn set_horizontal_text_scaling(&mut self, scale_percent: f32) -> Result<(), Self::ErrorType> {
        self.current_state_mut()?.text_state.horizontal_scaling = scale_percent;
        Ok(())
    }

    fn set_text_leading(&mut self, leading: f32) -> Result<(), Self::ErrorType> {
        Err(PdfCanvasError::NotImplemented(format!(
            "set_text_leading TL: {}",
            leading
        )))
    }

    fn set_font_and_size(&mut self, font_name: &str, size: f32) -> Result<(), Self::ErrorType> {
        self.current_state_mut()?.text_state.font_size = size;

        let resources = self
            .page
            .resources
            .as_ref()
            .ok_or(PdfCanvasError::MissingPageResources)?;

        if let Some(font) = resources.fonts.get(font_name) {
            self.current_state_mut()?.text_state.font = Some(font);
            return Ok(());
        }

        if let Some(resources) = self.current_state()?.resources
            && let Some(font) = resources.fonts.get(font_name)
        {
            self.current_state_mut()?.text_state.font = Some(font);
            return Ok(());
        }

        Err(PdfCanvasError::FontNotFound(font_name.to_string()))
    }

    fn set_text_rendering_mode(&mut self, _mode: TextRenderingMode) -> Result<(), Self::ErrorType> {
        Err(PdfCanvasError::NotImplemented(
            "set_text_rendering_mode".into(),
        ))
    }

    fn set_text_rise(&mut self, rise: f32) -> Result<(), Self::ErrorType> {
        self.current_state_mut()?.text_state.rise = rise;
        Ok(())
    }
}

impl<T: CanvasBackend> TextShowingOps for PdfCanvas<'_, T> {
    fn show_text(&mut self, text: &[u8]) -> Result<(), Self::ErrorType> {
        let text_state = &self.current_state()?.text_state;
        let current_font = text_state.font.ok_or(PdfCanvasError::NoCurrentFont)?;
        match current_font.subtype {
            FontSubType::Type3 => {
                let type3_font = current_font.type3_font.as_ref().ok_or_else(|| {
                    PdfCanvasError::MissingType3FontData(current_font.base_font.clone())
                })?;
                let mut renderer = Type3FontRenderer::new(
                    self,
                    text_state.font_size,
                    text_state.horizontal_scaling,
                    text_state.rise,
                    self.current_state()?.transform,
                    text_state.matrix,
                    type3_font,
                )?;
                renderer.render_text(text)
            }
            FontSubType::Type1 => {
                if let Some(type1_font) = current_font.type1_font.as_ref() {
                    // Limit immutable borrow by copying needed values into locals.
                    let (
                        font_size,
                        hscale,
                        tm,
                        rise,
                        word_spacing,
                        char_spacing,
                        current_transform,
                    ) = {
                        let st = &self.current_state()?.text_state;
                        let transform = self.current_state()?.transform;
                        (
                            st.font_size,
                            st.horizontal_scaling,
                            st.matrix,
                            st.rise,
                            st.word_spacing,
                            st.character_spacing,
                            transform,
                        )
                    };

                    let mut renderer = Type1FontRenderer::new(
                        self,
                        type1_font,
                        font_size,
                        hscale,
                        tm,
                        current_transform,
                        rise,
                    )
                    .with_spacing(word_spacing, char_spacing);
                    renderer.render_text(text)
                } else {
                    Err(PdfCanvasError::NotImplemented(
                        "Type1 font data missing".to_string(),
                    ))
                }
            }
            _ => {
                let mut renderer = TrueTypeFontRenderer::new(
                    self,
                    current_font,
                    text_state.font_size,
                    text_state.horizontal_scaling,
                    text_state.matrix,
                    self.current_state()?.transform,
                    text_state.rise,
                    text_state.word_spacing,
                    text_state.character_spacing,
                )?;
                renderer.render_text(text);
                Ok(())
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
                    println!("Text: {}", value);
                }
                TextElement::Adjustment { amount }=> {
                    let amount = (*amount) / 1000.0;
                    let state = self.current_state_mut()?;
                    let tx = - amount * state.text_state.font_size * state.text_state.horizontal_scaling;
                    state.text_state.matrix.translate(tx, 0.0);
                    println!("Adjustment: {}", amount);
                }
                TextElement::HexString { value } => {
                    println!("HexString: {}", String::from_utf8_lossy(value));
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
