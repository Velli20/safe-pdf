//! FemtoVG drawing; unsupported soft masks fail before painting begins.

use femtovg::{Canvas, Color, FillRule, Paint, Path};
use pdf_canvas::{
    CanvasPath,
    canvas_backend::{CanvasBackend, Shader},
    error::PdfCanvasError,
    mask_layer::MaskLayer,
    stroke_style::StrokeStyle,
};
use pdf_graphics::{BlendMode, Image, PathFillType, pdf_path::PathVerb};

/// Converts PDF path verbs into equivalent FemtoVG path geometry.
fn to_femtovg_path(pdf_path: &CanvasPath<'_>) -> Result<Path, PdfCanvasError> {
    let mut path = Path::new();
    for verb in pdf_path.verbs() {
        match &verb? {
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
    Ok(path)
}

/// Borrows a FemtoVG canvas for PDF drawing.
pub struct CanvasImpl<'a> {
    /// Native target whose state is managed by FemtoVG.
    pub canvas: &'a mut Canvas<femtovg::renderer::WGPURenderer>,
}

impl CanvasBackend for CanvasImpl<'_> {
    /// Fills the supplied device-space path using the requested paint and fill rule.
    fn fill_path(
        &mut self,
        path: &CanvasPath<'_>,
        fill_type: PathFillType,
        color: pdf_graphics::color::Color,
        _shader: Option<&Shader>,
        _blend_mode: Option<pdf_graphics::BlendMode>,
    ) -> Result<(), PdfCanvasError> {
        let path = to_femtovg_path(path)?;

        let mut fill_paint = Paint::color(Color::rgbf(color.r, color.g, color.b));
        fill_paint.set_anti_alias(true);
        match fill_type {
            PathFillType::Winding => fill_paint.set_fill_rule(FillRule::NonZero),
            PathFillType::EvenOdd => fill_paint.set_fill_rule(FillRule::EvenOdd),
        }
        self.canvas.fill_path(&path, &fill_paint);
        Ok(())
    }

    /// Strokes device-space geometry using the supplied width and stroke style.
    fn stroke_path(
        &mut self,
        path: &CanvasPath<'_>,
        color: pdf_graphics::color::Color,
        line_width: f32,
        stroke_style: &StrokeStyle,
        _shader: Option<&Shader>,
        _blend_mode: Option<pdf_graphics::BlendMode>,
    ) -> Result<(), PdfCanvasError> {
        let path = to_femtovg_path(path)?;

        let mut stroke_paint = Paint::color(Color::rgbf(color.r, color.g, color.b));
        stroke_paint.set_anti_alias(true);
        stroke_paint.set_line_width(line_width);
        stroke_paint.set_line_cap(match stroke_style.line_cap {
            pdf_graphics::LineCap::Butt => femtovg::LineCap::Butt,
            pdf_graphics::LineCap::Round => femtovg::LineCap::Round,
            pdf_graphics::LineCap::Square => femtovg::LineCap::Square,
        });
        stroke_paint.set_line_join(match stroke_style.line_join {
            pdf_graphics::LineJoin::Miter => femtovg::LineJoin::Miter,
            pdf_graphics::LineJoin::Round => femtovg::LineJoin::Round,
            pdf_graphics::LineJoin::Bevel => femtovg::LineJoin::Bevel,
        });
        stroke_paint.set_miter_limit(stroke_style.miter_limit);
        self.canvas.stroke_path(&path, &stroke_paint);
        Ok(())
    }

    /// Returns the logical canvas width used by PDF painting.
    fn width(&self) -> f32 {
        self.canvas.width() as f32
    }

    /// Returns the logical canvas height used by PDF painting.
    fn height(&self) -> f32 {
        self.canvas.height() as f32
    }

    /// Accepts the clip operator; FemtoVG path clipping is not yet implemented.
    fn set_clip_region(
        &mut self,
        _path: &CanvasPath<'_>,
        mode: PathFillType,
    ) -> Result<(), PdfCanvasError> {
        // let mut path = to_femtovg_path(path)?;
        match mode {
            PathFillType::Winding => {}
            PathFillType::EvenOdd => {}
        }
        Ok(())
    }

    /// Saves the current clipping and drawing state for a matching restore.
    fn save(&mut self) -> Result<(), PdfCanvasError> {
        self.canvas.save();
        Ok(())
    }

    /// Restores the most recently saved drawing state.
    fn restore(&mut self) -> Result<(), PdfCanvasError> {
        self.canvas.restore();
        Ok(())
    }

    /// Accepts image placement; image rendering is not yet implemented by this backend.
    fn draw_image_rect(
        &mut self,
        _image: &Image,
        _blend_mode: Option<BlendMode>,
        _dest_rect: pdf_graphics::rect::Rect,
        _image_rotation: Option<f32>,
    ) -> Result<(), PdfCanvasError> {
        // Not yet implemented in femtovg backend
        Ok(())
    }

    /// Invokes pass-through masks directly and rejects unsupported coverage modes before painting.
    fn with_mask_layer<F>(&mut self, mask: &MaskLayer, paint: F) -> Result<(), PdfCanvasError>
    where
        F: FnOnce(&mut Self) -> Result<(), PdfCanvasError>,
    {
        if mask.is_passthrough() {
            paint(self)
        } else {
            Err(PdfCanvasError::UnsupportedFeature(
                "FemtoVG soft masks".into(),
            ))
        }
    }
}
