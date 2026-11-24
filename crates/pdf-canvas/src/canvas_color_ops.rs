use pdf_color_space::color_space::ColorSpace;
use pdf_graphics::color::Color;

use crate::{
    canvas_backend::CanvasBackend, current_color::PaintTarget, error::PdfCanvasError,
    pdf_canvas::PdfCanvas,
};

impl<B: CanvasBackend> PdfCanvas<'_, B> {
    /// Handles `G`/`g`, `RG`/`rg` and `K`/`k`: selects a device color space and a color in it.
    pub(crate) fn set_device_color(
        &mut self,
        target: PaintTarget,
        space: ColorSpace,
        color: Color,
    ) -> Result<(), PdfCanvasError> {
        self.state.set_paint_color(target, color);
        let current = self.state.current_color_mut(target);
        current.pattern = None;
        current.space = space;
        Ok(())
    }

    /// Handles `SC`/`sc` and `SCN`/`scn` without a pattern: sets the current color.
    pub(crate) fn set_color(
        &mut self,
        target: PaintTarget,
        components: &[f32],
    ) -> Result<(), PdfCanvasError> {
        let current = self.state.current_color_mut(target);
        current.pattern = None;
        let color = current.space.color_from_operands(components)?;
        self.state.set_paint_color(target, color);
        Ok(())
    }

    /// Handles `SCN`/`scn` with a pattern name: selects the pattern, and the color of an
    /// uncolored one.
    ///
    /// # Errors
    ///
    /// Returns an error if the components do not fit the color space, or if the pattern is
    /// not found in the resources.
    pub(crate) fn set_pattern_color(
        &mut self,
        target: PaintTarget,
        components: &[f32],
        pattern_name: &[u8],
    ) -> Result<(), PdfCanvasError> {
        if !components.is_empty() {
            let current = self.state.current_color_mut(target);
            current.pattern = None;
            let space = current.space.pattern_component_space();
            let color = space.color_from_operands(components)?;
            self.state.set_paint_color(target, color);
        }

        let Some(pattern) = self
            .state
            .resources
            .as_ref()
            .and_then(|resources| resources.pattern(pattern_name))
        else {
            return Err(PdfCanvasError::PatternNotFound(
                String::from_utf8_lossy(pattern_name).into_owned(),
            ));
        };
        self.state.current_color_mut(target).pattern = Some(pattern);
        Ok(())
    }

    /// Handles `CS`/`cs`: selects a color space by name.
    ///
    /// # Errors
    ///
    /// Returns an error if the name is neither reserved nor found in the resources.
    pub(crate) fn set_color_space(
        &mut self,
        target: PaintTarget,
        name: &[u8],
    ) -> Result<(), PdfCanvasError> {
        self.state.current_color_mut(target).pattern = None;

        let space = match ColorSpace::from_reserved_name(name) {
            // Per PDF spec §8.6.8 selecting a device space also sets the current color to
            // its initial value.
            Some(ColorSpace::DeviceCMYK) => {
                let black = Color::from_cmyk(0.0, 0.0, 0.0, 1.0);
                return self.set_device_color(target, ColorSpace::DeviceCMYK, black);
            }
            Some(space @ (ColorSpace::DeviceGray | ColorSpace::DeviceRGB)) => {
                return self.set_device_color(target, space, Color::from_gray(0.0));
            }
            reserved => reserved.or_else(|| self.state.resources.as_ref()?.color_space(name)),
        };
        let Some(space) = space else {
            return Err(PdfCanvasError::ColorSpaceNotFound(
                String::from_utf8_lossy(name).into_owned(),
            ));
        };
        self.state.current_color_mut(target).space = space;
        Ok(())
    }
}
