use pdf_content_stream::pdf_operator_backend::ColorOps;

use crate::{error::PdfCanvasError, pdf_canvas::PdfCanvas};
use pdf_graphics::color::Color;

impl ColorOps for PdfCanvas<'_> {
    fn set_stroking_color_space(&mut self, _name: &str) -> Result<(), Self::ErrorType> {
        self.current_state_mut()?.stroke_pattern = None;
        Ok(())
    }

    fn set_non_stroking_color_space(&mut self, _name: &str) -> Result<(), Self::ErrorType> {
        self.current_state_mut()?.fill_pattern = None;
        Ok(())
    }

    fn set_stroking_color(&mut self, components: &[f32]) -> Result<(), Self::ErrorType> {
        // Map component arrays to a concrete color in the current graphics state.
        // Supported forms (per current backend support):
        // - 1 component: Gray (g)
        // - 3 components: RGB (r g b)
        // - 4 components: CMYK (c m y k)
        // Any time an explicit color is set, clear an active pattern.
        let state = self.current_state_mut()?;
        match *components {
            [g] => {
                state.stroke_color = Color::from_gray(g);
            }
            [r, g, b] => {
                state.stroke_color = Color::from_rgb(r, g, b);
            }
            [c, m, y, k] => {
                state.stroke_color = Color::from_cmyk(c, m, y, k);
            }
            _ => {
                return Err(PdfCanvasError::NotImplemented(format!(
                    "set_stroking_color expects 1 (Gray), 3 (RGB), or 4 (CMYK) components; got {:?}",
                    components
                )));
            }
        }
        state.stroke_pattern = None;
        Ok(())
    }

    fn set_non_stroking_color(&mut self, components: &[f32]) -> Result<(), Self::ErrorType> {
        // Same component mapping as set_stroking_color for the fill color.
        // Explicit color selection disables any active pattern.
        let state = self.current_state_mut()?;
        match *components {
            [g] => {
                state.fill_color = Color::from_gray(g);
            }
            [r, g, b] => {
                state.fill_color = Color::from_rgb(r, g, b);
            }
            [c, m, y, k] => {
                state.fill_color = Color::from_cmyk(c, m, y, k);
            }
            _ => {
                return Err(PdfCanvasError::NotImplemented(format!(
                    "set_non_stroking_color expects 1 (Gray), 3 (RGB), or 4 (CMYK) components; got {:?}",
                    components
                )));
            }
        }
        state.fill_pattern = None;
        Ok(())
    }

    fn set_non_stroking_color_extended(
        &mut self,
        components: &[f32],
        pattern_name: &str,
    ) -> Result<(), Self::ErrorType> {
        if !components.is_empty() {
            self.set_non_stroking_color(components)?;
        }

        self.set_fill_pattern(pattern_name)
    }

    fn set_stroking_color_extended(
        &mut self,
        components: &[f32],
        pattern_name: &str,
    ) -> Result<(), Self::ErrorType> {
        if !components.is_empty() {
            self.set_stroking_color(components)?;
        }

        self.set_stroke_pattern(pattern_name)
    }

    fn set_stroking_gray(&mut self, gray: f32) -> Result<(), Self::ErrorType> {
        let state = self.current_state_mut()?;
        state.stroke_color = Color::from_gray(gray);
        state.stroke_pattern = None;
        Ok(())
    }

    fn set_non_stroking_gray(&mut self, gray: f32) -> Result<(), Self::ErrorType> {
        let state = self.current_state_mut()?;
        state.fill_color = Color::from_gray(gray);
        state.fill_pattern = None;
        Ok(())
    }

    fn set_stroking_rgb(&mut self, r: f32, g: f32, b: f32) -> Result<(), Self::ErrorType> {
        let state = self.current_state_mut()?;
        state.stroke_color = Color::from_rgb(r, g, b);
        state.stroke_pattern = None;
        Ok(())
    }

    fn set_non_stroking_rgb(&mut self, r: f32, g: f32, b: f32) -> Result<(), Self::ErrorType> {
        let state = self.current_state_mut()?;
        state.fill_color = Color::from_rgb(r, g, b);
        state.fill_pattern = None;
        Ok(())
    }

    fn set_stroking_cmyk(&mut self, c: f32, m: f32, y: f32, k: f32) -> Result<(), Self::ErrorType> {
        let state = self.current_state_mut()?;
        state.stroke_color = Color::from_cmyk(c, m, y, k);
        state.stroke_pattern = None;
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
        state.fill_pattern = None;
        Ok(())
    }
}
