use femtovg::{Canvas, Color, FillRule, Paint, Path};
use pdf_graphics::pdf_path::{PathVerb, PdfPath};
use pdf_graphics::{CanvasBackend, PathFillType};

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
    fn fill_path(
        &mut self,
        path: &PdfPath,
        fill_type: PathFillType,
        color: pdf_graphics::color::Color,
    ) {
        let mut path = to_femtovg_path(path);

        let mut fill_paint = Paint::color(Color::rgbf(color.r, color.g, color.b));
        fill_paint.set_anti_alias(true);
        match fill_type {
            PathFillType::Winding => fill_paint.set_fill_rule(FillRule::NonZero),
            PathFillType::EvenOdd => fill_paint.set_fill_rule(FillRule::EvenOdd),
        }
        self.canvas.fill_path(&mut path, &fill_paint)
    }

    fn stroke_path(&mut self, path: &PdfPath, color: pdf_graphics::color::Color, line_width: f32) {
        let mut path = to_femtovg_path(path);

        let mut stroke_paint = Paint::color(Color::rgbf(color.r, color.g, color.b));
        stroke_paint.set_anti_alias(true);
        stroke_paint.set_line_width(line_width);
        self.canvas.stroke_path(&mut path, &stroke_paint)
    }

    fn width(&self) -> f32 {
        self.canvas.width() as f32
    }

    fn height(&self) -> f32 {
        self.canvas.height() as f32
    }

    fn set_clip_region(&mut self, path: &PdfPath, mode: PathFillType) {
        // let mut path = to_femtovg_path(path);
        match mode {
            PathFillType::Winding => {}
            PathFillType::EvenOdd => {}
        }
    }
}
