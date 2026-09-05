use pdf_color_space::color_space::ColorSpace;
use pdf_color_space::error::ColorSpaceError;
use pdf_content_stream_operators::pdf_operator_backend::ColorOps;

use crate::{
    canvas_backend::CanvasBackend, canvas_state::CanvasState, error::PdfCanvasError,
    pdf_canvas::PdfCanvas,
};
use pdf_graphics::color::Color;

fn apply_content_stream_color(
    color_space: &ColorSpace,
    components: &[f32],
) -> Result<Color, ColorSpaceError> {
    // Honor the declared color space whenever its component count is valid.
    match color_space.apply(components) {
        Ok(color) => Ok(color),
        // Some malformed content streams use SC/sc/SCN/scn operands whose count
        // identifies a different device color space than the active one. Recover
        // only from that arity mismatch; other color spaces and errors stay strict.
        Err(err @ ColorSpaceError::InsufficientComponents(_, _))
            if color_space.is_device_space() =>
        {
            Color::from_device_components(components).ok_or(err)
        }
        Err(err) => Err(err),
    }
}

impl<B: CanvasBackend> ColorOps for PdfCanvas<'_, B> {
    type ErrorType = PdfCanvasError;
    fn set_stroking_color_space(&mut self, name: &[u8]) -> Result<(), Self::ErrorType> {
        self.set_color_space(name, true)
    }

    fn set_non_stroking_color_space(&mut self, name: &[u8]) -> Result<(), Self::ErrorType> {
        self.set_color_space(name, false)
    }

    fn set_stroking_color(&mut self, components: &[f32]) -> Result<(), Self::ErrorType> {
        let state = self.current_state_mut()?;
        state.stroke_pattern = None;

        let Some(color_space) = &state.stroke_color_space else {
            return Err(PdfCanvasError::ColorSpaceNotSet);
        };

        state.paint.stroke_color = apply_content_stream_color(color_space, components)?;
        Ok(())
    }

    fn set_non_stroking_color(&mut self, components: &[f32]) -> Result<(), Self::ErrorType> {
        let state = self.current_state_mut()?;
        state.fill_pattern = None;

        let Some(color_space) = &state.fill_color_space else {
            return Err(PdfCanvasError::ColorSpaceNotSet);
        };

        state.paint.fill_color = apply_content_stream_color(color_space, components)?;
        Ok(())
    }

    fn set_non_stroking_color_extended(
        &mut self,
        components: &[f32],
        pattern_name: &[u8],
    ) -> Result<(), Self::ErrorType> {
        if !components.is_empty() {
            let state = self.current_state_mut()?;
            state.fill_pattern = None;
            let Some(cs) = state.fill_color_space else {
                return Err(PdfCanvasError::ColorSpaceNotSet);
            };
            // For uncolored tiling patterns the components belong to the underlying
            // color space wrapped inside Pattern, not to the Pattern space itself.
            let color = match cs {
                ColorSpace::Pattern(Some(inner_cs)) => {
                    apply_content_stream_color(inner_cs, components)?
                }
                _ => apply_content_stream_color(cs, components)?,
            };
            state.paint.fill_color = color;
        }

        self.set_fill_pattern(pattern_name)
    }

    fn set_stroking_color_extended(
        &mut self,
        components: &[f32],
        pattern_name: &[u8],
    ) -> Result<(), Self::ErrorType> {
        if !components.is_empty() {
            let state = self.current_state_mut()?;
            state.stroke_pattern = None;
            let Some(cs) = state.stroke_color_space else {
                return Err(PdfCanvasError::ColorSpaceNotSet);
            };
            // For uncolored tiling patterns the components belong to the underlying
            // color space wrapped inside Pattern, not to the Pattern space itself.
            let color = match cs {
                ColorSpace::Pattern(Some(inner_cs)) => {
                    apply_content_stream_color(inner_cs, components)?
                }
                _ => apply_content_stream_color(cs, components)?,
            };
            state.paint.stroke_color = color;
        }

        self.set_stroke_pattern(pattern_name)
    }

    fn set_stroking_gray(&mut self, gray: f32) -> Result<(), Self::ErrorType> {
        let state = self.current_state_mut()?;
        state.paint.stroke_color = Color::from_gray(gray);
        state.stroke_pattern = None;
        state.stroke_color_space = Some(&CanvasState::DEVICE_GRAY_COLOR_SPACE);
        Ok(())
    }

    fn set_non_stroking_gray(&mut self, gray: f32) -> Result<(), Self::ErrorType> {
        let state = self.current_state_mut()?;
        state.paint.fill_color = Color::from_gray(gray);
        state.fill_pattern = None;
        state.fill_color_space = Some(&CanvasState::DEVICE_GRAY_COLOR_SPACE);
        Ok(())
    }

    fn set_stroking_rgb(&mut self, r: f32, g: f32, b: f32) -> Result<(), Self::ErrorType> {
        let state = self.current_state_mut()?;
        state.paint.stroke_color = Color::from_rgb(r, g, b);
        state.stroke_pattern = None;
        state.stroke_color_space = Some(&CanvasState::DEVICE_RGB_COLOR_SPACE);
        Ok(())
    }

    fn set_non_stroking_rgb(&mut self, r: f32, g: f32, b: f32) -> Result<(), Self::ErrorType> {
        let state = self.current_state_mut()?;
        state.paint.fill_color = Color::from_rgb(r, g, b);
        state.fill_pattern = None;
        state.fill_color_space = Some(&CanvasState::DEVICE_RGB_COLOR_SPACE);
        Ok(())
    }

    fn set_stroking_cmyk(&mut self, c: f32, m: f32, y: f32, k: f32) -> Result<(), Self::ErrorType> {
        let state = self.current_state_mut()?;
        state.paint.stroke_color = Color::from_cmyk(c, m, y, k);
        state.stroke_pattern = None;
        state.stroke_color_space = Some(&CanvasState::DEVICE_CMYK_COLOR_SPACE);
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
        state.paint.fill_color = Color::from_cmyk(c, m, y, k);
        state.fill_pattern = None;
        state.fill_color_space = Some(&CanvasState::DEVICE_CMYK_COLOR_SPACE);
        Ok(())
    }
}
