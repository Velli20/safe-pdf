use pdf_color_space::color_space::ColorSpace;
use pdf_color_space::error::ColorSpaceError;

use crate::{canvas_backend::CanvasBackend, error::PdfCanvasError, pdf_canvas::PdfCanvas};
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

impl<B: CanvasBackend> PdfCanvas<'_, B> {
    /// Handles `SC`/`SCN` without a pattern: sets the stroking color.
    pub(crate) fn set_stroking_color(&mut self, components: &[f32]) -> Result<(), PdfCanvasError> {
        let state = &mut self.state;
        state.stroke_pattern = None;

        state.paint.set_stroke_color(apply_content_stream_color(
            &state.stroke_color_space,
            components,
        )?);
        Ok(())
    }

    /// Handles `sc`/`scn` without a pattern: sets the non-stroking color.
    pub(crate) fn set_non_stroking_color(
        &mut self,
        components: &[f32],
    ) -> Result<(), PdfCanvasError> {
        let state = &mut self.state;
        state.fill_pattern = None;

        state.paint.set_fill_color(apply_content_stream_color(
            &state.fill_color_space,
            components,
        )?);
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
            let cs = state.fill_color_space.as_ref();
            // For uncolored tiling patterns the components belong to the underlying
            // color space wrapped inside Pattern, not to the Pattern space itself.
            let color = match cs {
                ColorSpace::Pattern(Some(inner_cs)) => {
                    apply_content_stream_color(inner_cs, components)?
                }
                _ => apply_content_stream_color(cs, components)?,
            };
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
            let cs = state.stroke_color_space.as_ref();
            // For uncolored tiling patterns the components belong to the underlying
            // color space wrapped inside Pattern, not to the Pattern space itself.
            let color = match cs {
                ColorSpace::Pattern(Some(inner_cs)) => {
                    apply_content_stream_color(inner_cs, components)?
                }
                _ => apply_content_stream_color(cs, components)?,
            };
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
