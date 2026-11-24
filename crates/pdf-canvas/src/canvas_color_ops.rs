use pdf_color_space::color_space::ColorSpace;

use crate::{canvas_backend::CanvasBackend, error::PdfCanvasError, pdf_canvas::PdfCanvas};
use pdf_graphics::color::Color;

impl<B: CanvasBackend> PdfCanvas<'_, B> {
    /// Handles `SC`/`SCN` without a pattern: sets the stroking color.
    pub(crate) fn set_stroking_color(&mut self, components: &[f32]) -> Result<(), PdfCanvasError> {
        let state = &mut self.state;
        state.stroke_pattern = None;
        let color = state.stroke_color_space.color_from_operands(components)?;
        state.paint.set_stroke_color(color);
        Ok(())
    }

    /// Handles `sc`/`scn` without a pattern: sets the non-stroking color.
    pub(crate) fn set_non_stroking_color(
        &mut self,
        components: &[f32],
    ) -> Result<(), PdfCanvasError> {
        let state = &mut self.state;
        state.fill_pattern = None;
        let color = state.fill_color_space.color_from_operands(components)?;
        state.paint.set_fill_color(color);
        Ok(())
    }

    /// Handles `scn` with a pattern name: sets the non-stroking pattern.
    pub(crate) fn set_non_stroking_color_extended(
        &mut self,
        components: &[f32],
        pattern_name: &[u8],
    ) -> Result<(), PdfCanvasError> {
        if !components.is_empty() {
            let state = &mut self.state;
            state.fill_pattern = None;
            let space = state.fill_color_space.pattern_component_space();
            let color = space.color_from_operands(components)?;
            state.paint.set_fill_color(color);
        }

        self.set_fill_pattern(pattern_name)
    }

    /// Handles `SCN` with a pattern name: sets the stroking pattern.
    pub(crate) fn set_stroking_color_extended(
        &mut self,
        components: &[f32],
        pattern_name: &[u8],
    ) -> Result<(), PdfCanvasError> {
        if !components.is_empty() {
            let state = &mut self.state;
            state.stroke_pattern = None;
            let space = state.stroke_color_space.pattern_component_space();
            let color = space.color_from_operands(components)?;
            state.paint.set_stroke_color(color);
        }

        self.set_stroke_pattern(pattern_name)
    }

    /// Handles `G`: sets a DeviceGray stroking color.
    pub(crate) fn set_stroking_gray(&mut self, gray: f32) -> Result<(), PdfCanvasError> {
        let state = &mut self.state;
        state.paint.set_stroke_color(Color::from_gray(gray));
        state.stroke_pattern = None;
        state.select_device_color_space(true, ColorSpace::DeviceGray);
        Ok(())
    }

    /// Handles `g`: sets a DeviceGray non-stroking color.
    pub(crate) fn set_non_stroking_gray(&mut self, gray: f32) -> Result<(), PdfCanvasError> {
        let state = &mut self.state;
        state.paint.set_fill_color(Color::from_gray(gray));
        state.fill_pattern = None;
        state.select_device_color_space(false, ColorSpace::DeviceGray);
        Ok(())
    }

    /// Handles `RG`: sets a DeviceRGB stroking color.
    pub(crate) fn set_stroking_rgb(
        &mut self,
        r: f32,
        g: f32,
        b: f32,
    ) -> Result<(), PdfCanvasError> {
        let state = &mut self.state;
        state.paint.set_stroke_color(Color::from_rgb(r, g, b));
        state.stroke_pattern = None;
        state.select_device_color_space(true, ColorSpace::DeviceRGB);
        Ok(())
    }

    /// Handles `rg`: sets a DeviceRGB non-stroking color.
    pub(crate) fn set_non_stroking_rgb(
        &mut self,
        r: f32,
        g: f32,
        b: f32,
    ) -> Result<(), PdfCanvasError> {
        let state = &mut self.state;
        state.paint.set_fill_color(Color::from_rgb(r, g, b));
        state.fill_pattern = None;
        state.select_device_color_space(false, ColorSpace::DeviceRGB);
        Ok(())
    }

    /// Handles `K`: sets a DeviceCMYK stroking color.
    pub(crate) fn set_stroking_cmyk(
        &mut self,
        c: f32,
        m: f32,
        y: f32,
        k: f32,
    ) -> Result<(), PdfCanvasError> {
        let state = &mut self.state;
        state.paint.set_stroke_color(Color::from_cmyk(c, m, y, k));
        state.stroke_pattern = None;
        state.select_device_color_space(true, ColorSpace::DeviceCMYK);
        Ok(())
    }

    /// Handles `k`: sets a DeviceCMYK non-stroking color.
    pub(crate) fn set_non_stroking_cmyk(
        &mut self,
        c: f32,
        m: f32,
        y: f32,
        k: f32,
    ) -> Result<(), PdfCanvasError> {
        let state = &mut self.state;
        state.paint.set_fill_color(Color::from_cmyk(c, m, y, k));
        state.fill_pattern = None;
        state.select_device_color_space(false, ColorSpace::DeviceCMYK);
        Ok(())
    }
}
