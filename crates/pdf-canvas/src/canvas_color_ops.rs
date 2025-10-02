use pdf_content_stream::pdf_operator_backend::ColorOps;

use crate::{canvas_backend::CanvasBackend, error::PdfCanvasError, pdf_canvas::PdfCanvas};
use pdf_graphics::color::Color;

impl<T: CanvasBackend> ColorOps for PdfCanvas<'_, T> {
    fn set_stroking_color_space(&mut self, _name: &str) -> Result<(), Self::ErrorType> {
        self.current_state_mut()?.pattern = None;
        Ok(())
    }

    fn set_non_stroking_color_space(&mut self, _name: &str) -> Result<(), Self::ErrorType> {
        self.current_state_mut()?.pattern = None;
        Ok(())
    }

    fn set_stroking_color(&mut self, _components: &[f32]) -> Result<(), Self::ErrorType> {
        Err(PdfCanvasError::NotImplemented("set_stroking_color".into()))
    }

    fn set_non_stroking_color_extended(
        &mut self,
        components: &[f32],
        pattern_name: &str,
    ) -> Result<(), Self::ErrorType> {
        if !components.is_empty() {
            return Err(PdfCanvasError::NotImplemented(
                "set_non_stroking_color_extended with components".into(),
            ));
        }

        self.set_pattern(pattern_name)
    }

    fn set_stroking_color_extended(
        &mut self,
        components: &[f32],
        pattern_name: &str,
    ) -> Result<(), Self::ErrorType> {
        if !components.is_empty() {
            return Err(PdfCanvasError::NotImplemented(
                "set_stroking_color_extended with components".into(),
            ));
        }

        self.set_pattern(pattern_name)
    }

    fn set_non_stroking_color(&mut self, _components: &[f32]) -> Result<(), Self::ErrorType> {
        Err(PdfCanvasError::NotImplemented(
            "set_non_stroking_color".into(),
        ))
    }

    fn set_stroking_gray(&mut self, _gray: f32) -> Result<(), Self::ErrorType> {
        let state = self.current_state_mut()?;
        state.stroke_color = Color::from_gray(_gray);
        state.pattern = None;
        Ok(())
    }

    fn set_non_stroking_gray(&mut self, _gray: f32) -> Result<(), Self::ErrorType> {
        let state = self.current_state_mut()?;
        state.fill_color = Color::from_gray(_gray);
        state.pattern = None;
        Ok(())
    }

    fn set_stroking_rgb(&mut self, r: f32, g: f32, b: f32) -> Result<(), Self::ErrorType> {
        let state = self.current_state_mut()?;
        state.stroke_color = Color::from_rgb(r, g, b);
        state.pattern = None;
        Ok(())
    }

    fn set_non_stroking_rgb(&mut self, r: f32, g: f32, b: f32) -> Result<(), Self::ErrorType> {
        let state = self.current_state_mut()?;
        state.fill_color = Color::from_rgb(r, g, b);
        state.pattern = None;
        Ok(())
    }

    fn set_stroking_cmyk(&mut self, c: f32, m: f32, y: f32, k: f32) -> Result<(), Self::ErrorType> {
        let state = self.current_state_mut()?;
        state.stroke_color = Color::from_cmyk(c, m, y, k);
        state.pattern = None;
        Ok(())
    }

    fn set_non_stroking_cmyk(
        &mut self,
        c: f32,
        m: f32,
        y: f32,
        k: f32,
    ) -> Result<(), Self::ErrorType> {
        let state = self.current_state_mut()?;
        state.fill_color = Color::from_cmyk(c, m, y, k);
        state.pattern = None;
        Ok(())
    }
}
