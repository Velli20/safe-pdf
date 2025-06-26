use pdf_graphics::{
    PathFillType as PdfPathFillType,
    canvas_backend::CanvasBackend,
    color::Color as PdfColor,
    pdf_path::{PathVerb, PdfPath as PdfGraphicsPath},
    transform::Transform,
};
use skia_safe::{
    AlphaType, ClipOp, Color, Color4f, ColorType, Data, ImageInfo, Matrix, Paint, Path as SkiaPath,
    PathFillType as SkiaPathFillType, Rect, image::Image as SkiaImage,
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

fn to_skia_matrix(transform: &Transform) -> Matrix {
    Matrix::new_all(
        transform.sx,
        transform.kx,
        transform.tx,
        transform.ky,
        transform.sy,
        transform.ty,
        0.0,
        0.0,
        1.0,
    )
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
        // self.stroke_path(path,  PdfColor::from_rgba(0.7, 0.5, 0.3, 1.0),2.0);
        sk_path.set_fill_type(sk_fill_type);
        self.canvas.save();
        self.canvas
            .clip_path(&sk_path, ClipOp::Intersect, Some(true));
    }

    fn reset_clip(&mut self) {
        self.canvas.restore();
    }

    fn draw_image(
        &mut self,
        image: &[u8],
        is_jpeg: bool,
        width: f32,
        height: f32,
        bits_per_component: u32,
        transform: &Transform,
    ) {
        let skia_image = if is_jpeg {
            // Data is JPEG encoded, use from_encoded
            SkiaImage::from_encoded(Data::new_copy(&image))
        } else {
            // Assume raw pixel data (e.g., after FlateDecode or no filter)
            if width == 0.0 || height == 0.0 {
                return;
            }

            // A robust implementation needs to inspect the PDF's ColorSpace entry.
            // Here, we deduce it from the number of components, assuming 8 bits per component.
            if bits_per_component != 8 {
                eprintln!(
                    "Unsupported bits per component for raw image: {}",
                    bits_per_component
                );
                return;
            }

            let num_components = image.len() / (width as usize * height as usize);

            let (color_type, pixel_data) = match num_components {
                4 => (ColorType::RGBA8888, Data::new_copy(&image)),
                3 => {
                    // Skia doesn't have a direct 24-bit RGB format. We convert to 32-bit RGBA.
                    let mut padded_data = Vec::with_capacity(width as usize * height as usize * 4);
                    for rgb in image.chunks_exact(3) {
                        padded_data.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 0xFF]);
                    }
                    (ColorType::RGBA8888, Data::new_copy(&padded_data))
                }
                1 => (ColorType::Gray8, Data::new_copy(&image)),
                _ => {
                    eprintln!(
                        "Unsupported number of components for raw image: {}",
                        num_components
                    );
                    return;
                }
            };

            let image_info = ImageInfo::new(
                (width as i32, height as i32),
                color_type,
                AlphaType::Unpremul, // PDF images are typically unpremultiplied
                None,
            );

            let row_bytes = width as usize * image_info.bytes_per_pixel();

            skia_safe::images::raster_from_data(&image_info, pixel_data, row_bytes)
        };

        if let Some(skia_image) = skia_image {
            let skia_matrix = to_skia_matrix(transform);
            let paint = Paint::default();
            self.canvas.save();
            self.canvas.concat(&skia_matrix);
            // The image is defined in a 1x1 unit square in user space.
            let dest_rect = Rect::from_xywh(0.0, 0.0, 1.0, 1.0);
            self.canvas
                .draw_image_rect(&skia_image, None, dest_rect, &paint);
            self.canvas.restore();
        } else {
            eprintln!("Failed to create Skia image from image XObject data");
        }
    }
}
