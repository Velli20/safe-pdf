use pdf_graphics::{
    PathFillType as PdfPathFillType,
    canvas_backend::CanvasBackend,
    color::Color as PdfColor,
    pdf_path::{PathVerb, PdfPath as PdfGraphicsPath},
    transform::Transform,
};
use skia_safe::{
    AlphaType, ClipOp, Color4f, ColorType, Data, ImageInfo, Matrix, Paint, Path as SkiaPath,
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
        -transform.sy,
        transform.ty,
        0.0,
        0.0,
        1.0,
    )
}

fn get_skia_image_data(
    image: &[u8],
    width: usize,
    height: usize,
    bits_per_component: u32,
    smask: Option<&[u8]>,
) -> Option<(ColorType, Data)> {
    if bits_per_component != 8 {
        eprintln!("Unsupported bits per component: {}", bits_per_component);
        return None;
    }
    let num_pixels = width * height;
    let num_components = image.len() / num_pixels;
    match num_components {
        4 => {
            if let Some(smask_data) = smask {
                let mut modified = Vec::with_capacity(image.len());
                for (i, rgba) in image.chunks_exact(4).enumerate() {
                    let smask_alpha = smask_data.get(i).copied().unwrap_or(255);
                    let new_alpha = (u16::from(rgba[3]) * u16::from(smask_alpha) / 255) as u8;
                    modified.extend_from_slice(&[rgba[0], rgba[1], rgba[2], new_alpha]);
                }
                Some((ColorType::RGBA8888, Data::new_copy(&modified)))
            } else {
                Some((ColorType::RGBA8888, Data::new_copy(image)))
            }
        }
        3 => {
            let mut padded = Vec::with_capacity(num_pixels * 4);
            if let Some(smask_data) = smask {
                for (i, rgb) in image.chunks_exact(3).enumerate() {
                    let alpha = smask_data.get(i).copied().unwrap_or(255);
                    padded.extend_from_slice(&[rgb[0], rgb[1], rgb[2], alpha]);
                }
            } else {
                for rgb in image.chunks_exact(3) {
                    padded.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
                }
            }
            Some((ColorType::RGBA8888, Data::new_copy(&padded)))
        }
        1 => Some((ColorType::Gray8, Data::new_copy(image))),
        _ => {
            eprintln!("Unsupported number of components: {}", num_components);
            None
        }
    }
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
        smask: Option<&[u8]>,
    ) {
        if width == 0.0 || height == 0.0 {
            return;
        }

        let skia_image = if is_jpeg {
            SkiaImage::from_encoded(Data::new_copy(image))
        } else {
            let (w, h) = (width as usize, height as usize);
            let (color_type, pixel_data) =
                match get_skia_image_data(image, w, h, bits_per_component, smask) {
                    Some(data) => data,
                    None => return,
                };
            let image_info =
                ImageInfo::new((w as i32, h as i32), color_type, AlphaType::Unpremul, None);
            let row_bytes = w * image_info.bytes_per_pixel();
            skia_safe::images::raster_from_data(&image_info, pixel_data, row_bytes)
        };

        if let Some(skia_image) = skia_image {
            let skia_matrix = to_skia_matrix(transform);
            let paint = Paint::default();
            self.canvas.save();
            self.canvas.concat(&skia_matrix);
            let dest_rect = Rect::from_xywh(0.0, -1.0, 1.0, 1.0);
            self.canvas
                .draw_image_rect(&skia_image, None, dest_rect, &paint);
            self.canvas.restore();
        } else {
            eprintln!("Failed to create Skia image from image XObject data");
        }
    }
}
