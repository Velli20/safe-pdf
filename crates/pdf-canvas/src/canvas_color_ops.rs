use pdf_color_space::color_space::ColorSpace;
use pdf_color_space::error::ColorSpaceError;
use pdf_content_stream_operators::pdf_operator_backend::ColorOps;

use crate::{
    canvas_backend::CanvasBackend, canvas_state::CanvasState, error::PdfCanvasError,
    pdf_canvas::PdfCanvas,
};
use pdf_graphics::color::Color;

fn color_from_generic_components(components: &[f32]) -> Option<Color> {
    match *components {
        [gray] => Some(Color::from_gray(gray)),
        [r, g, b] => Some(Color::from_rgb(r, g, b)),
        [c, m, y, k] => Some(Color::from_cmyk(c, m, y, k)),
        _ => None,
    }
}

fn apply_content_stream_color(
    color_space: &ColorSpace,
    components: &[f32],
) -> Result<Color, ColorSpaceError> {
    match color_space.apply(components) {
        Ok(color) => Ok(color),
        Err(err @ ColorSpaceError::InsufficientComponents(_, _))
            if color_space.is_device_space() =>
        {
            color_from_generic_components(components).ok_or(err)
        }
        Err(err) => Err(err),
    }
}

impl<B: CanvasBackend> ColorOps for PdfCanvas<'_, B> {
    type ErrorType = PdfCanvasError;
    fn set_stroking_color_space(&mut self, name: &str) -> Result<(), Self::ErrorType> {
        self.set_color_space(name, true)
    }

    fn set_non_stroking_color_space(&mut self, name: &str) -> Result<(), Self::ErrorType> {
        self.set_color_space(name, false)
    }

    fn set_stroking_color(&mut self, components: &[f32]) -> Result<(), Self::ErrorType> {
        let state = self.current_state_mut()?;
        state.stroke_pattern = None;

        let Some(color_space) = &state.stroke_color_space else {
            return Err(PdfCanvasError::ColorSpaceNotSet);
        };

        state.stroke_color = apply_content_stream_color(color_space, components)?;
        Ok(())
    }

    fn set_non_stroking_color(&mut self, components: &[f32]) -> Result<(), Self::ErrorType> {
        let state = self.current_state_mut()?;
        state.fill_pattern = None;

        let Some(color_space) = &state.fill_color_space else {
            return Err(PdfCanvasError::ColorSpaceNotSet);
        };

        state.fill_color = apply_content_stream_color(color_space, components)?;
        Ok(())
    }

    fn set_non_stroking_color_extended(
        &mut self,
        components: &[f32],
        pattern_name: &str,
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
            state.fill_color = color;
        }

        self.set_fill_pattern(pattern_name)
    }

    fn set_stroking_color_extended(
        &mut self,
        components: &[f32],
        pattern_name: &str,
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
            state.stroke_color = color;
        }

        self.set_stroke_pattern(pattern_name)
    }

    fn set_stroking_gray(&mut self, gray: f32) -> Result<(), Self::ErrorType> {
        let state = self.current_state_mut()?;
        state.stroke_color = Color::from_gray(gray);
        state.stroke_pattern = None;
        state.stroke_color_space = Some(&CanvasState::DEVICE_GRAY_COLOR_SPACE);
        Ok(())
    }

    fn set_non_stroking_gray(&mut self, gray: f32) -> Result<(), Self::ErrorType> {
        let state = self.current_state_mut()?;
        state.fill_color = Color::from_gray(gray);
        state.fill_pattern = None;
        state.fill_color_space = Some(&CanvasState::DEVICE_GRAY_COLOR_SPACE);
        Ok(())
    }

    fn set_stroking_rgb(&mut self, r: f32, g: f32, b: f32) -> Result<(), Self::ErrorType> {
        let state = self.current_state_mut()?;
        state.stroke_color = Color::from_rgb(r, g, b);
        state.stroke_pattern = None;
        state.stroke_color_space = Some(&CanvasState::DEVICE_RGB_COLOR_SPACE);
        Ok(())
    }

    fn set_non_stroking_rgb(&mut self, r: f32, g: f32, b: f32) -> Result<(), Self::ErrorType> {
        let state = self.current_state_mut()?;
        state.fill_color = Color::from_rgb(r, g, b);
        state.fill_pattern = None;
        state.fill_color_space = Some(&CanvasState::DEVICE_RGB_COLOR_SPACE);
        Ok(())
    }

    fn set_stroking_cmyk(&mut self, c: f32, m: f32, y: f32, k: f32) -> Result<(), Self::ErrorType> {
        let state = self.current_state_mut()?;
        state.stroke_color = Color::from_cmyk(c, m, y, k);
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
        state.fill_color = Color::from_cmyk(c, m, y, k);
        state.fill_pattern = None;
        state.fill_color_space = Some(&CanvasState::DEVICE_CMYK_COLOR_SPACE);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use pdf_color_space::error::ColorSpaceError;
    use pdf_content_stream_operators::pdf_operator_backend::ColorOps;
    use pdf_graphics::{
        BlendMode, MaskMode, PathFillType, color::Color, pdf_path::PdfPath, rect::Rect,
        transform::Transform,
    };
    use pdf_page::page::PdfPage;
    use std::sync::Arc;

    use crate::{
        canvas_backend::{CanvasBackend, Image, Shader},
        recording_canvas::RecordingCanvas,
    };

    use super::PdfCanvas;

    #[derive(Default)]
    struct TestCanvas;

    impl CanvasBackend for TestCanvas {
        fn fill_path(
            &mut self,
            _path: &PdfPath,
            _fill_type: PathFillType,
            _color: Color,
            _shader: &Option<Shader>,
            _blend_mode: Option<BlendMode>,
        ) -> Result<(), crate::error::PdfCanvasError> {
            Ok(())
        }

        fn stroke_path(
            &mut self,
            _path: &PdfPath,
            _color: Color,
            _line_width: f32,
            _stroke_style: &crate::stroke_style::StrokeStyle,
            _shader: &Option<Shader>,
            _blend_mode: Option<BlendMode>,
        ) -> Result<(), crate::error::PdfCanvasError> {
            Ok(())
        }

        fn set_clip_region(
            &mut self,
            _path: &PdfPath,
            _mode: PathFillType,
        ) -> Result<(), crate::error::PdfCanvasError> {
            Ok(())
        }

        fn width(&self) -> f32 {
            100.0
        }

        fn height(&self) -> f32 {
            100.0
        }

        fn save(&mut self) -> Result<(), crate::error::PdfCanvasError> {
            Ok(())
        }

        fn restore(&mut self) -> Result<(), crate::error::PdfCanvasError> {
            Ok(())
        }

        fn draw_image_rect(
            &mut self,
            _image: &Image<'_>,
            _blend_mode: Option<BlendMode>,
            _dest_rect: Rect,
            _image_rotation: Option<f32>,
        ) -> Result<(), crate::error::PdfCanvasError> {
            Ok(())
        }

        fn draw_inline_image(
            &mut self,
            _image: &Image<'_>,
            _blend_mode: Option<BlendMode>,
            _dest_rect: Rect,
            _image_rotation: Option<f32>,
        ) -> Result<(), crate::error::PdfCanvasError> {
            Ok(())
        }

        fn begin_mask_layer(
            &mut self,
            _mask: &Arc<RecordingCanvas>,
            _transform: &Transform,
            _mask_mode: MaskMode,
        ) -> Result<(), crate::error::PdfCanvasError> {
            Ok(())
        }

        fn end_mask_layer(
            &mut self,
            _mask: &Arc<RecordingCanvas>,
            _transform: &Transform,
            _mask_mode: MaskMode,
        ) -> Result<(), crate::error::PdfCanvasError> {
            Ok(())
        }
    }

    #[test]
    fn default_device_gray_scn_with_three_components_falls_back_to_rgb() {
        let page = PdfPage::default();
        let mut backend = TestCanvas;
        let mut canvas = PdfCanvas::new(&mut backend, &page, None).expect("canvas should build");

        canvas
            .set_non_stroking_color(&[0.294_118, 0.019_608, 0.196_078])
            .expect("rgb fallback should apply");

        assert_eq!(
            canvas
                .current_state()
                .expect("state should exist")
                .fill_color,
            Color::from_rgb(0.294_118, 0.019_608, 0.196_078)
        );
    }

    #[test]
    fn device_rgb_scn_with_three_components_still_uses_active_space() {
        let page = PdfPage::default();
        let mut backend = TestCanvas;
        let mut canvas = PdfCanvas::new(&mut backend, &page, None).expect("canvas should build");

        canvas
            .set_non_stroking_color_space("DeviceRGB")
            .expect("device rgb should resolve");
        canvas
            .set_non_stroking_color(&[0.0, 0.0, 0.0])
            .expect("rgb color should apply");

        assert_eq!(
            canvas
                .current_state()
                .expect("state should exist")
                .fill_color,
            Color::from_rgb(0.0, 0.0, 0.0)
        );
    }

    #[test]
    fn invalid_generic_operand_count_still_returns_component_error() {
        let page = PdfPage::default();
        let mut backend = TestCanvas;
        let mut canvas = PdfCanvas::new(&mut backend, &page, None).expect("canvas should build");

        let err = canvas
            .set_non_stroking_color(&[0.2, 0.4])
            .expect_err("two generic components should still fail");

        assert!(matches!(
            err,
            crate::error::PdfCanvasError::ColorSpaceError(ColorSpaceError::InsufficientComponents(
                1, 2
            ))
        ));
    }
}
