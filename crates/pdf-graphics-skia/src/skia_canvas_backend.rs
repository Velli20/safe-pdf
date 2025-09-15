use std::borrow::Cow;

use pdf_canvas::canvas_backend::{CanvasBackend, Image, Shader};
use pdf_graphics::{
    BlendMode, PathFillType,
    color::Color,
    pdf_path::{PathVerb, PdfPath},
    transform::Transform,
};

pub enum SurfaceContainer<'a> {
    Borrowed(&'a mut skia_safe::Surface),
    Owned(skia_safe::Surface),
}

impl SurfaceContainer<'_> {
    fn canvas(&mut self) -> &skia_safe::Canvas {
        match self {
            SurfaceContainer::Borrowed(surface) => surface.canvas(),
            SurfaceContainer::Owned(surface) => surface.canvas(),
        }
    }
}

pub struct SkiaCanvasBackend<'a> {
    pub surface: SurfaceContainer<'a>,
    pub width: f32,
    pub height: f32,
}

fn to_skia_image(image: &Image<'_>) -> Option<skia_safe::Image> {
    if image.encoding.as_deref() == Some("jpeg") {
        return skia_safe::Image::from_encoded(skia_safe::Data::new_copy(&image.data));
    }

    let (color_type, pixel_data) = get_skia_image_data(
        &image.data,
        image.width as usize,
        image.height as usize,
        8,
        &image.mask,
    )
    .unwrap();

    let image_info = skia_safe::ImageInfo::new(
        (image.width as i32, image.height as i32),
        color_type,
        skia_safe::AlphaType::Unpremul,
        None,
    );

    let row_bytes = image.width as usize * image_info.bytes_per_pixel();
    skia_safe::images::raster_from_data(&image_info, pixel_data, row_bytes)
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

/// Maps PDF BlendMode to Skia BlendMode
fn to_skia_blend_mode(mode: BlendMode) -> skia_safe::BlendMode {
    match mode {
        BlendMode::Normal => skia_safe::BlendMode::SrcOver,
        BlendMode::Multiply => skia_safe::BlendMode::Multiply,
        BlendMode::Screen => skia_safe::BlendMode::Screen,
        BlendMode::Overlay => skia_safe::BlendMode::Overlay,
        BlendMode::Darken => skia_safe::BlendMode::Darken,
        BlendMode::Lighten => skia_safe::BlendMode::Lighten,
        BlendMode::ColorDodge => skia_safe::BlendMode::ColorDodge,
        BlendMode::ColorBurn => skia_safe::BlendMode::ColorBurn,
        BlendMode::HardLight => skia_safe::BlendMode::HardLight,
        BlendMode::SoftLight => skia_safe::BlendMode::SoftLight,
        BlendMode::Difference => skia_safe::BlendMode::Difference,
        BlendMode::Exclusion => skia_safe::BlendMode::Exclusion,
        BlendMode::Hue => skia_safe::BlendMode::Hue,
        BlendMode::Saturation => skia_safe::BlendMode::Saturation,
        BlendMode::Color => skia_safe::BlendMode::Color,
        BlendMode::Luminosity => skia_safe::BlendMode::Luminosity,
        BlendMode::DestinationIn => skia_safe::BlendMode::DstIn,
    }
}

fn to_skia_shader(shader: &Shader) -> Option<skia_safe::Shader> {
    match shader {
        &Shader::LinearGradient {
            x0,
            y0,
            x1,
            y1,
            positions,
            colors,
        } => {
            // Prepare colors and positions for Skia
            let colors: Vec<skia_safe::Color> = colors
                .iter()
                .map(|color| skia_safe::Color4f::new(color.r, color.g, color.b, color.a).to_color())
                .collect();

            // Create the Skia gradient shader
            skia_safe::Shader::linear_gradient(
                (skia_safe::Point::new(x0, y0), skia_safe::Point::new(x1, y1)),
                skia_safe::gradient_shader::GradientShaderColors::Colors(&colors),
                Some(positions),
                skia_safe::TileMode::Clamp,
                None,
                None,
            )
        }
        &Shader::RadialGradient {
            start_x,
            start_y,
            start_r,
            end_x,
            end_y,
            end_r,
            positions,
            colors,
            transform,
        } => {
            // Prepare colors and positions for Skia
            let colors: Vec<skia_safe::Color> = colors
                .iter()
                .map(|color| skia_safe::Color4f::new(color.r, color.g, color.b, color.a).to_color())
                .collect();

            let mat = if let Some(m) = transform {
                to_skia_matrix(&m)
            } else {
                skia_safe::Matrix::new_identity()
            };

            skia_safe::Shader::two_point_conical_gradient(
                skia_safe::Point::new(start_x, start_y),
                start_r,
                skia_safe::Point::new(end_x, end_y),
                end_r,
                skia_safe::gradient_shader::GradientShaderColors::Colors(&colors),
                Some(positions),
                skia_safe::TileMode::Clamp,
                None,
                Some(&mat),
            )
        }
        Shader::TilingPatternImage {
            image,
            transform: _,
            x_step: _,
            y_step: _,
        } => {
            let image = to_skia_image(image).unwrap();
            image.to_shader(
                (skia_safe::TileMode::Repeat, skia_safe::TileMode::Repeat),
                skia_safe::SamplingOptions::default(),
                None,
            )
        }
    }
}

/// Creates a Skia Paint object for a given color and style.
fn make_paint(
    color: Color,
    style: skia_safe::paint::Style,
    width: Option<f32>,
    blend_mode: Option<BlendMode>,
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
    if let Some(mode) = blend_mode {
        paint.set_blend_mode(to_skia_blend_mode(mode));
    }
    paint
}

/// Converts image data to Skia's expected format.
fn get_skia_image_data(
    image: &[u8],
    width: usize,
    height: usize,
    bits_per_component: u32,
    smask: &Option<Cow<'_, [u8]>>,
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
            skia_safe::ColorType::Alpha8,
            skia_safe::Data::new_copy(image),
        )),
        _ => {
            eprintln!("Unsupported number of components: {}", num_components);
            None
        }
    }
}

impl CanvasBackend for SkiaCanvasBackend<'_> {
    type MaskType = Self;

    fn fill_path(
        &mut self,
        path: &PdfPath,
        fill_type: PathFillType,
        color: Color,
        shader: &Option<Shader>,
        blend_mode: Option<BlendMode>,
    ) {
        let mut sk_path = to_skia_path(path);
        sk_path.set_fill_type(to_skia_fill_type(fill_type));
        let mut paint = make_paint(color, skia_safe::paint::Style::Fill, None, blend_mode);
        if let Some(shader) = shader {
            if let Some(shader) = to_skia_shader(shader) {
                paint.set_shader(shader);
            }
        }

        self.surface.canvas().draw_path(&sk_path, &paint);
    }

    fn stroke_path(
        &mut self,
        path: &PdfPath,
        color: Color,
        line_width: f32,
        shader: &Option<Shader>,
        blend_mode: Option<BlendMode>,
    ) {
        let sk_path = to_skia_path(path);
        let mut paint = make_paint(
            color,
            skia_safe::paint::Style::Stroke,
            Some(line_width),
            blend_mode,
        );
        if let Some(shader) = shader {
            if let Some(shader) = to_skia_shader(shader) {
                paint.set_shader(shader);
            }
        }

        self.surface.canvas().draw_path(&sk_path, &paint);
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
        self.surface.canvas().save();
        self.surface
            .canvas()
            .clip_path(&sk_path, skia_safe::ClipOp::Intersect, Some(true));
    }

    fn reset_clip(&mut self) {
        self.surface.canvas().restore();
    }

    fn draw_image(&mut self, image: &Image<'_>, blend_mode: Option<BlendMode>) {
        if image.width == 0 || image.height == 0 {
            return;
        }

        let Some(skia_image) = to_skia_image(image) else {
            eprintln!("Failed to create Skia image from image XObject data");
            return;
        };

        let skia_matrix = to_skia_matrix(&image.transform);
        let mut paint = skia_safe::Paint::default();
        if let Some(mode) = blend_mode {
            paint.set_blend_mode(to_skia_blend_mode(mode));
        }
        self.surface.canvas().save();
        self.surface.canvas().concat(&skia_matrix);
        let dest_rect = skia_safe::Rect::from_xywh(0.0, -1.0, 1.0, 1.0);
        self.surface
            .canvas()
            .draw_image_rect(&skia_image, None, dest_rect, &paint);
        self.surface.canvas().restore();
    }

    fn new_mask_layer(&mut self, width: f32, height: f32) -> Box<Self::MaskType> {
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

        let mask_canvas = Self {
            surface: SurfaceContainer::Owned(surface),
            width,
            height,
        };

        Box::new(mask_canvas)
    }

    fn begin_mask_layer(&mut self, _mask: &mut Self::MaskType) {
        self.surface.canvas().clear(skia_safe::Color::WHITE);
        let rec = skia_safe::canvas::SaveLayerRec::default();
        self.surface.canvas().save_layer(&rec);
    }

    fn end_mask_layer(&mut self, mask: &mut Self::MaskType, transform: &Transform) {
        let mask_image = match mask.surface {
            SurfaceContainer::Borrowed(ref mut s) => unsafe {
                s.canvas().surface().unwrap().image_snapshot()
            },
            SurfaceContainer::Owned(ref mut s) => unsafe {
                s.canvas().surface().unwrap().image_snapshot()
            },
        };

        let mat = to_skia_matrix(transform);
        self.surface.canvas().concat(&mat);
        let mut paint = skia_safe::Paint::default();
        paint.set_blend_mode(skia_safe::BlendMode::DstIn);
        let height = -mask_image.height() as f32;
        self.surface
            .canvas()
            .draw_image(mask_image, (0.0, height), Some(&paint));
        self.surface.canvas().restore();
    }

    fn image_snapshot(&mut self) -> Image<'static> {
        let image = unsafe { self.surface.canvas().surface().unwrap().image_snapshot() };
        let pixmap = image.peek_pixels().expect("Could not peek pixels");
        let info = pixmap.info().clone();
        let bytes = pixmap.bytes().unwrap().to_vec();

        Image {
            data: Cow::Owned(bytes),
            width: self.width as u32,
            height: self.height as u32,
            bytes_per_pixel: Some(info.bytes_per_pixel() as u32),
            encoding: None,
            transform: Transform::identity(),
            mask: None,
        }
    }
}
