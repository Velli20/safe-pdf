use skia_safe::{Color4f, Paint, Path as SkiaPath, PathFillType as SkiaPathFillType};

use pdf_graphics::{
    CanvasBackend, PathFillType as PdfPathFillType,
    color::Color as PdfColor,
    pdf_path::{PathVerb, PdfPath as PdfGraphicsPath},
};

pub struct SkiaCanvasBackend<'a> {
    pub canvas: &'a skia_safe::Canvas,
    pub width: f32,
    pub height: f32,
}

fn to_skia_path(pdf_path: &PdfGraphicsPath) -> SkiaPath {
    let mut path = SkiaPath::new();
    for verb in &pdf_path.verbs {
        match verb {
            PathVerb::MoveTo { x, y } => path.move_to((*x, *y)),
            PathVerb::LineTo { x, y } => path.line_to((*x, *y)),
            PathVerb::CubicTo {
                x1,
                y1,
                x2,
                y2,
                x3,
                y3,
            } => path.cubic_to((*x1, *y1), (*x2, *y2), (*x3, *y3)),
            PathVerb::Close => path.close(),
            PathVerb::QuadTo { x1, y1, x2, y2 } => path.quad_to((*x1, *y1), (*x2, *y2)),
        };
    }
    path
}

impl<'a> CanvasBackend for SkiaCanvasBackend<'a> {
    fn fill_path(&mut self, path: &PdfGraphicsPath, fill_type: PdfPathFillType, color: PdfColor) {
        let mut sk_path = to_skia_path(path);
        let sk_color = Color4f::new(color.r, color.g, color.b, color.a);
        let mut paint = Paint::new(sk_color, None);
        paint.set_anti_alias(true);
        paint.set_style(skia_safe::paint::Style::Fill);

        let sk_fill_type = match fill_type {
            PdfPathFillType::Winding => SkiaPathFillType::Winding,
            PdfPathFillType::EvenOdd => SkiaPathFillType::EvenOdd,
        };
        sk_path.set_fill_type(sk_fill_type);

        self.canvas.draw_path(&sk_path, &paint);
    }

    fn stroke_path(&mut self, path: &PdfGraphicsPath, color: PdfColor, line_width: f32) {
        let sk_path = to_skia_path(path);
        let sk_color = Color4f::new(color.r, color.g, color.b, color.a);
        let mut paint = Paint::new(sk_color, None);
        paint.set_anti_alias(true);
        paint.set_style(skia_safe::paint::Style::Stroke);
        paint.set_stroke_width(line_width);

        self.canvas.draw_path(&sk_path, &paint);
    }

    fn width(&self) -> f32 {
        self.width
    }

    fn height(&self) -> f32 {
        self.height
    }

    fn set_clip_region(&mut self, path: &PdfGraphicsPath, mode: PdfPathFillType) {
        let mut sk_path = to_skia_path(path);
        let sk_fill_type = match mode {
            PdfPathFillType::Winding => SkiaPathFillType::Winding,
            PdfPathFillType::EvenOdd => SkiaPathFillType::EvenOdd,
        };
        sk_path.set_fill_type(sk_fill_type);
        self.canvas.clip_path(&sk_path, None, Some(true)); // Default op: Intersect, anti_alias: true
    }
}
