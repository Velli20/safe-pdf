use femtovg::{Canvas, Color, FillRule, Paint, Path};
use pdf_canvas::{
    canvas_backend::{CanvasBackend, Image, Shader},
    recording_canvas::RecordingCanvas,
};
use pdf_graphics::{
    BlendMode, MaskMode, PathFillType,
    pdf_path::{PathVerb, PdfPath},
};

#[derive(Debug, thiserror::Error)]
pub enum FemtovgCanvasBackendError {}

fn to_femtovg_path(pdf_path: &PdfPath) -> Path {
    let mut path = Path::new();
    for verb in &pdf_path.verbs {
        match verb {
            PathVerb::MoveTo { x, y } => {
                path.move_to(*x, *y);
            }
            PathVerb::LineTo { x, y } => {
                path.line_to(*x, *y);
            }
            PathVerb::CubicTo {
                x1,
                y1,
                x2,
                y2,
                x3,
                y3,
            } => {
                path.bezier_to(*x1, *y1, *x2, *y2, *x3, *y3);
            }
            PathVerb::Close => {
                path.close();
            }
            PathVerb::QuadTo { x1, y1, x2, y2 } => {
                path.quad_to(*x1, *y1, *x2, *y2);
            }
        }
    }
    path
}

pub struct CanvasImpl<'a> {
    pub canvas: &'a mut Canvas<femtovg::renderer::WGPURenderer>,
}

impl CanvasBackend for CanvasImpl<'_> {
    type ErrorType = FemtovgCanvasBackendError;

    fn fill_path(
        &mut self,
        path: &PdfPath,
        fill_type: PathFillType,
        color: pdf_graphics::color::Color,
        _shader: &Option<Shader>,
        _blend_mode: Option<pdf_graphics::BlendMode>,
    ) -> Result<(), Self::ErrorType> {
        let path = to_femtovg_path(path);

        let mut fill_paint = Paint::color(Color::rgbf(color.r, color.g, color.b));
        fill_paint.set_anti_alias(true);
        match fill_type {
            PathFillType::Winding => fill_paint.set_fill_rule(FillRule::NonZero),
            PathFillType::EvenOdd => fill_paint.set_fill_rule(FillRule::EvenOdd),
        }
        self.canvas.fill_path(&path, &fill_paint);
        Ok(())
    }

    fn stroke_path(
        &mut self,
        path: &PdfPath,
        color: pdf_graphics::color::Color,
        line_width: f32,
        _shader: &Option<Shader>,
        _blend_mode: Option<pdf_graphics::BlendMode>,
    ) -> Result<(), Self::ErrorType> {
        let path = to_femtovg_path(path);

        let mut stroke_paint = Paint::color(Color::rgbf(color.r, color.g, color.b));
        stroke_paint.set_anti_alias(true);
        stroke_paint.set_line_width(line_width);
        self.canvas.stroke_path(&path, &stroke_paint);
        Ok(())
    }

    fn width(&self) -> f32 {
        self.canvas.width() as f32
    }

    fn height(&self) -> f32 {
        self.canvas.height() as f32
    }

    fn set_clip_region(
        &mut self,
        _path: &PdfPath,
        mode: PathFillType,
    ) -> Result<(), Self::ErrorType> {
        // let mut path = to_femtovg_path(path);
        match mode {
            PathFillType::Winding => {}
            PathFillType::EvenOdd => {}
        }
        Ok(())
    }

    fn reset_clip(&mut self) -> Result<(), Self::ErrorType> {
        Ok(())
    }

    fn draw_image(
        &mut self,
        _image: &Image<'_>,
        _blend_mode: Option<BlendMode>,
    ) -> Result<(), Self::ErrorType> {
        // Not yet implemented in femtovg backend
        Ok(())
    }

    fn begin_mask_layer(
        &mut self,
        _mask: &RecordingCanvas,
        _transform: &pdf_graphics::transform::Transform,
        _mask_mode: MaskMode,
    ) -> Result<(), Self::ErrorType> {
        // Not yet implemented in femtovg backend
        Ok(())
    }

    fn end_mask_layer(
        &mut self,
        _mask: &RecordingCanvas,
        _transform: &pdf_graphics::transform::Transform,
        _mask_mode: MaskMode,
    ) -> Result<(), Self::ErrorType> {
        // Not yet implemented in femtovg backend
        Ok(())
    }
}
