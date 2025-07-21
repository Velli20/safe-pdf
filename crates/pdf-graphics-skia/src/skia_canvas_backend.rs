use pdf_graphics::{
    PathFillType,
    canvas_backend::{CanvasBackend, Shader},
    color::Color,
    pdf_path::{PathVerb, PdfPath},
    transform::Transform,
};

pub struct SkiaCanvasBackend<'a> {
    pub canvas: &'a skia_safe::Canvas,
    pub width: f32,
    pub height: f32,
}

pub struct SkiaMaskCanvas {
    surface: skia_safe::Surface,
    width: f32,
    height: f32,
    mask_image: Option<skia_safe::Image>,
}

/// Converts a PdfPath to a Skia Path.
fn to_skia_path(pdf_path: &PdfPath) -> skia_safe::Path {
    let mut path = skia_safe::Path::new();
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

/// Converts a PDF Transform to a Skia Matrix.
fn to_skia_matrix(transform: &Transform) -> skia_safe::Matrix {
    skia_safe::Matrix::new_all(
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

/// Converts a PDF fill type to a Skia fill type.
fn to_skia_fill_type(fill_type: PathFillType) -> skia_safe::PathFillType {
    match fill_type {
        PathFillType::Winding => skia_safe::PathFillType::Winding,
        PathFillType::EvenOdd => skia_safe::PathFillType::EvenOdd,
    }
}

fn to_skia_shader(shader: &Shader) -> Option<skia_safe::Shader> {
    match shader {
        Shader::LinearGradient {
            x0,
            y0,
            x1,
            y1,
            stops,
        } => {
            // Prepare colors and positions for Skia
            let mut colors = Vec::with_capacity(stops.len());
            let mut positions: Vec<f32> = Vec::with_capacity(stops.len());
            for (color, pos) in stops {
                colors.push(skia_safe::Color4f::new(color.r, color.g, color.b, color.a).to_color());
                positions.push(*pos);
            }

            // Create the Skia gradient shader
            skia_safe::Shader::linear_gradient(
                (
                    skia_safe::Point::new(*x0, *y0),
                    skia_safe::Point::new(*x1, *y1),
                ),
                skia_safe::gradient_shader::GradientShaderColors::Colors(&colors),
                Some(positions.as_slice()),
                skia_safe::TileMode::Clamp,
                None,
                None,
            )
        }
        Shader::RadialGradient {
            start_x,
            start_y,
            start_r,
            end_x,
            end_y,
            end_r,

            stops,
            transform,
        } => {
            // Prepare colors and positions for Skia
            let mut colors = Vec::with_capacity(stops.len());
            let mut positions: Vec<f32> = Vec::with_capacity(stops.len());
            for (color, pos) in stops {
                colors.push(skia_safe::Color4f::new(color.r, color.g, color.b, color.a).to_color());
                positions.push(*pos);
            }

            let mat = if let Some(m) = transform {
                to_skia_matrix(m)
            } else {
                skia_safe::Matrix::new_identity()
            };

            skia_safe::Shader::two_point_conical_gradient(
                skia_safe::Point::new(*start_x, *start_y),
                *start_r,
                skia_safe::Point::new(*end_x, *end_y),
                *end_r,
                skia_safe::gradient_shader::GradientShaderColors::Colors(&colors),
                Some(positions.as_slice()),
                skia_safe::TileMode::Clamp,
                None,
                Some(&mat),
            )
        }
    }
}

/// Creates a Skia Paint object for a given color and style.
fn make_paint(
    color: Color,
    style: skia_safe::paint::Style,
    width: Option<f32>,
) -> skia_safe::Paint {
    let mut paint = skia_safe::Paint::new(
        skia_safe::Color4f::new(color.r, color.g, color.b, color.a),
        None,
    );
    paint.set_anti_alias(true);
    paint.set_style(style);
    if let Some(w) = width {
        paint.set_stroke_width(w);
    }
    paint
}

/// Converts image data to Skia's expected format.
fn get_skia_image_data(
    image: &[u8],
    width: usize,
    height: usize,
    bits_per_component: u32,
    smask: Option<&[u8]>,
) -> Option<(skia_safe::ColorType, skia_safe::Data)> {
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
                Some((
                    skia_safe::ColorType::RGBA8888,
                    skia_safe::Data::new_copy(&modified),
                ))
            } else {
                Some((
                    skia_safe::ColorType::RGBA8888,
                    skia_safe::Data::new_copy(image),
                ))
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
            Some((
                skia_safe::ColorType::RGBA8888,
                skia_safe::Data::new_copy(&padded),
            ))
        }
        1 => Some((
            skia_safe::ColorType::Gray8,
            skia_safe::Data::new_copy(image),
        )),
        _ => {
            eprintln!("Unsupported number of components: {}", num_components);
            None
        }
    }
}

impl<'a> CanvasBackend for SkiaCanvasBackend<'a> {
    type MaskType = SkiaMaskCanvas;

    fn fill_path(
        &mut self,
        path: &PdfPath,
        fill_type: PathFillType,
        color: Color,
        shader: &Option<Shader>,
    ) {
        let mut sk_path = to_skia_path(path);
        sk_path.set_fill_type(to_skia_fill_type(fill_type));
        let mut paint = make_paint(color, skia_safe::paint::Style::Fill, None);
        if let Some(shader) = shader {
            if let Some(shader) = to_skia_shader(shader) {
                paint.set_shader(shader);
            }
        }

        self.canvas.draw_path(&sk_path, &paint);
    }

    fn stroke_path(
        &mut self,
        path: &PdfPath,
        color: Color,
        line_width: f32,
        shader: &Option<Shader>,
    ) {
        let sk_path = to_skia_path(path);
        let mut paint = make_paint(color, skia_safe::paint::Style::Stroke, Some(line_width));
        if let Some(shader) = shader {
            if let Some(shader) = to_skia_shader(shader) {
                paint.set_shader(shader);
            }
        }

        self.canvas.draw_path(&sk_path, &paint);
    }

    fn width(&self) -> f32 {
        self.width
    }

    fn height(&self) -> f32 {
        self.height
    }

    fn set_clip_region(&mut self, path: &PdfPath, mode: PathFillType) {
        let mut sk_path = to_skia_path(path);
        sk_path.set_fill_type(to_skia_fill_type(mode));
        // self.stroke_path(path,  Color::from_rgba(0.7, 0.5, 0.3, 1.0),2.0);
        self.canvas.save();
        self.canvas
            .clip_path(&sk_path, skia_safe::ClipOp::Intersect, Some(true));
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
            skia_safe::Image::from_encoded(skia_safe::Data::new_copy(image))
        } else {
            let (w, h) = (width as usize, height as usize);
            let (color_type, pixel_data) =
                match get_skia_image_data(image, w, h, bits_per_component, smask) {
                    Some(data) => data,
                    None => return,
                };
            let image_info = skia_safe::ImageInfo::new(
                (w as i32, h as i32),
                color_type,
                skia_safe::AlphaType::Unpremul,
                None,
            );
            let row_bytes = w * image_info.bytes_per_pixel();
            skia_safe::images::raster_from_data(&image_info, pixel_data, row_bytes)
        };

        if let Some(skia_image) = skia_image {
            let skia_matrix = to_skia_matrix(transform);
            let paint = skia_safe::Paint::default();
            self.canvas.save();
            self.canvas.concat(&skia_matrix);
            let dest_rect = skia_safe::Rect::from_xywh(0.0, -1.0, 1.0, 1.0);
            self.canvas
                .draw_image_rect(&skia_image, None, dest_rect, &paint);
            self.canvas.restore();
        } else {
            eprintln!("Failed to create Skia image from image XObject data");
        }
    }

    fn create_mask(&mut self, width: f32, height: f32) -> Box<Self::MaskType> {
        // Create an ImageInfo describing the bitmap's properties for A8.
        let image_info = skia_safe::ImageInfo::new(
            (width as i32, height as i32),
            skia_safe::ColorType::Alpha8,
            skia_safe::AlphaType::Premul,
            Some(skia_safe::ColorSpace::new_srgb()),
        );

        // Create a new surface to draw your mask onto.
        let mut surface = skia_safe::surfaces::raster(&image_info, None, None).unwrap();
        let canvas = surface.canvas();

        // Clear the canvas to fully transparent (alpha 0)
        canvas.clear(skia_safe::Color::TRANSPARENT);

        let mask_canvas = SkiaMaskCanvas {
            surface,
            width,
            height,
            mask_image: None,
        };

        Box::new(mask_canvas)
    }

    fn enable_mask(&mut self, mask: &mut Self::MaskType) {
        let mask_image = mask.surface.image_snapshot();
        // let data = mask_image
        //     .encode(None, skia_safe::EncodedImageFormat::PNG, 100)
        //     .unwrap();
        // std::fs::write("masked_draw.png", data.as_bytes()).unwrap();

        mask.mask_image = Some(mask_image);
        self.canvas.clear(skia_safe::Color::WHITE);
        let rec = skia_safe::canvas::SaveLayerRec::default();
        self.canvas.save_layer(&rec);
    }

    fn finish_mask(&mut self, mask: &mut Self::MaskType, transform: &Transform) {
        let mat = to_skia_matrix(transform);
        self.canvas.concat(&mat);
        let mut paint = skia_safe::Paint::default();
        paint.set_blend_mode(skia_safe::BlendMode::DstIn);
        let mask_image = mask.mask_image.take().unwrap();
        let height = -mask_image.height() as f32;
        self.canvas
            .draw_image(mask_image, (0.0, height), Some(&paint));
        self.canvas.restore();
    }
}

impl<'a> CanvasBackend for SkiaMaskCanvas {
    type MaskType = SkiaMaskCanvas;

    fn fill_path(
        &mut self,
        path: &PdfPath,
        fill_type: PathFillType,
        color: Color,
        shader: &Option<Shader>,
    ) {
        let mut sk_path = to_skia_path(path);
        let sk_color = skia_safe::Color4f::new(color.r, color.g, color.b, color.a);
        let mut paint = skia_safe::Paint::new(sk_color, None);
        paint.set_anti_alias(true);
        paint.set_style(skia_safe::paint::Style::Fill);

        if let Some(shader) = shader {
            if let Some(shader) = to_skia_shader(shader) {
                paint.set_shader(shader);
            }
        }

        let sk_fill_type = match fill_type {
            PathFillType::Winding => skia_safe::PathFillType::Winding,
            PathFillType::EvenOdd => skia_safe::PathFillType::EvenOdd,
        };
        sk_path.set_fill_type(sk_fill_type);

        self.surface.canvas().draw_path(&sk_path, &paint);
    }

    fn stroke_path(
        &mut self,
        path: &PdfPath,
        color: Color,
        line_width: f32,
        shader: &Option<Shader>,
    ) {
        let sk_path = to_skia_path(path);
        let sk_color = skia_safe::Color4f::new(color.r, color.g, color.b, color.a);
        let mut paint = skia_safe::Paint::new(sk_color, None);
        paint.set_anti_alias(true);
        paint.set_style(skia_safe::paint::Style::Stroke);
        paint.set_stroke_width(line_width);
        if let Some(shader) = shader {
            if let Some(shader) = to_skia_shader(shader) {
                paint.set_shader(shader);
            }
        }
        self.surface.canvas().draw_path(&sk_path, &paint);
    }

    fn set_clip_region(&mut self, path: &PdfPath, mode: PathFillType) {
        let mut sk_path = to_skia_path(path);
        let sk_fill_type = match mode {
            PathFillType::Winding => skia_safe::PathFillType::Winding,
            PathFillType::EvenOdd => skia_safe::PathFillType::EvenOdd,
        };
        // self.stroke_path(path,  Color::from_rgba(0.7, 0.5, 0.3, 1.0),2.0);
        sk_path.set_fill_type(sk_fill_type);
        self.surface.canvas().save();
        self.surface
            .canvas()
            .clip_path(&sk_path, skia_safe::ClipOp::Intersect, Some(true));
    }

    fn width(&self) -> f32 {
        self.width
    }

    fn height(&self) -> f32 {
        self.height
    }

    fn reset_clip(&mut self) {
        self.surface.canvas().restore();
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
            skia_safe::Image::from_encoded(skia_safe::Data::new_copy(image))
        } else {
            let (w, h) = (width as usize, height as usize);
            let (color_type, pixel_data) =
                match get_skia_image_data(image, w, h, bits_per_component, smask) {
                    Some(data) => data,
                    None => return,
                };
            let image_info = skia_safe::ImageInfo::new(
                (w as i32, h as i32),
                color_type,
                skia_safe::AlphaType::Unpremul,
                None,
            );
            let row_bytes = w * image_info.bytes_per_pixel();
            skia_safe::images::raster_from_data(&image_info, pixel_data, row_bytes)
        };

        if let Some(skia_image) = skia_image {
            let skia_matrix = to_skia_matrix(transform);
            let paint = skia_safe::Paint::default();
            self.surface.canvas().save();
            self.surface.canvas().concat(&skia_matrix);
            let dest_rect = skia_safe::Rect::from_xywh(0.0, -1.0, 1.0, 1.0);
            self.surface
                .canvas()
                .draw_image_rect(&skia_image, None, dest_rect, &paint);
            self.surface.canvas().restore();
        } else {
            eprintln!("Failed to create Skia image from image XObject data");
        }
    }

    fn create_mask(&mut self, _width: f32, _height: f32) -> Box<Self::MaskType> {
        unimplemented!("Nested masks are not supported for SkiaMaskCanvas");
    }
    fn enable_mask(&mut self, _mask: &mut Self::MaskType) {
        unimplemented!("Nested masks are not supported for SkiaMaskCanvas");
    }
    fn finish_mask(&mut self, _mask: &mut Self::MaskType, _transform: &Transform) {
        unimplemented!("Nested masks are not supported for SkiaMaskCanvas");
    }
}
