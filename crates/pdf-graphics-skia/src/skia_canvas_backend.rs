use std::sync::Arc;

use pdf_canvas::{
    canvas_backend::{CanvasBackend, Image, Shader},
    error::PdfCanvasError,
    recording_canvas::RecordingCanvas,
};
use pdf_graphics::{
    BlendMode, MaskMode, PathFillType, PixelFormat,
    color::Color,
    pdf_path::{PathVerb, PdfPath},
    transform::Transform,
};

#[derive(Debug, thiserror::Error)]
pub enum SkiaCanvasBackendError {
    #[error("failed to allocate raster surface for {kind} ({width}x{height})")]
    SurfaceAllocationFailed {
        kind: &'static str,
        width: u32,
        height: u32,
    },
    #[error("invalid image dimensions: {width}x{height}")]
    InvalidImageDimensions { width: usize, height: usize },
    #[error("failed to decode image with encoding: {encoding}")]
    ImageDecodeFailed { encoding: &'static str },
    #[error("failed to create skia raster image from data ({width}x{height})")]
    RasterImageCreationFailed { width: usize, height: usize },
    #[error("failed to create shader: {shader}")]
    ShaderCreationFailed { shader: &'static str },
}

impl From<SkiaCanvasBackendError> for PdfCanvasError {
    fn from(e: SkiaCanvasBackendError) -> Self {
        PdfCanvasError::BackendError(e.to_string())
    }
}

pub struct SkiaCanvasBackend<'a> {
    pub surface: &'a mut skia_safe::Surface,
    pub width: f32,
    pub height: f32,
}

/// Renders a recorded mask into an 8-bit alpha Skia image.
///
/// This allocates an A8 raster surface matching the recording canvas size,
/// replays the recorded drawing operations into it using the current
/// Skia backend implementation, and returns an `Image` snapshot of the
/// rasterized mask. The result can be used as a shader or for compositing
/// operations that expect an alpha mask.
fn to_skia_a8_mask_image(
    recording_canvas: &RecordingCanvas,
) -> Result<skia_safe::Image, PdfCanvasError> {
    let width = recording_canvas.width();
    let height = recording_canvas.height();
    let info = skia_safe::ImageInfo::new(
        (width as i32, height as i32),
        skia_safe::ColorType::RGBA8888,
        skia_safe::AlphaType::Unpremul,
        None,
    );
    let Some(mut surface) = skia_safe::surfaces::raster(&info, None, None) else {
        return Err(SkiaCanvasBackendError::SurfaceAllocationFailed {
            kind: "mask",
            width: width as u32,
            height: height as u32,
        }
        .into());
    };

    let mut mask_backend = SkiaCanvasBackend {
        surface: &mut surface,
        width: recording_canvas.width(),
        height: recording_canvas.height(),
    };
    recording_canvas.replay(&mut mask_backend)?;
    Ok(surface.image_snapshot())
}

/// Convert a PDF `Image` into a Skia `Image`.
///
/// Handles different color component configurations:
/// - 4 components (RGBA): Pass through directly.
/// - 3 components (RGB): Expand to RGBA with full alpha.
/// - 1 component (Grayscale): Use Gray8 format.
///
/// # Parameters
///
/// - `image`: PDF image descriptor containing pixel data, dimensions, optional
///   encoding, and optional soft mask.
///
/// # Returns
///
/// - A Skia `Image` ready to be drawn with `draw_image`/`draw_image_rect`.
fn to_skia_image(image: &Image<'_>) -> Result<skia_safe::Image, PdfCanvasError> {
    let width = image.width;
    let height = image.height;
    let pixel_format = image.pixel_format;

    if width == 0 || height == 0 {
        return Err(SkiaCanvasBackendError::InvalidImageDimensions { width, height }.into());
    }

    let color_type = match pixel_format {
        PixelFormat::RGBA8888 => skia_safe::ColorType::RGBA8888,
        PixelFormat::Gray8 => skia_safe::ColorType::Gray8,
    };

    let image_info = skia_safe::ImageInfo::new(
        (width as i32, height as i32),
        color_type,
        skia_safe::AlphaType::Unpremul,
        None,
    );

    let pixel_data = skia_safe::Data::new_copy(&image.data);

    let row_bytes = width * image_info.bytes_per_pixel();
    skia_safe::images::raster_from_data(&image_info, pixel_data, row_bytes)
        .ok_or_else(|| SkiaCanvasBackendError::RasterImageCreationFailed { width, height }.into())
}

/// Converts a PdfPath to a Skia Path.
fn to_skia_path(pdf_path: &PdfPath) -> skia_safe::Path {
    let mut builder = skia_safe::PathBuilder::new();
    for verb in &pdf_path.verbs {
        match verb {
            PathVerb::MoveTo { x, y } => {
                builder.move_to((*x, *y));
            }
            PathVerb::LineTo { x, y } => {
                builder.line_to((*x, *y));
            }
            PathVerb::CubicTo {
                x1,
                y1,
                x2,
                y2,
                x3,
                y3,
            } => {
                builder.cubic_to((*x1, *y1), (*x2, *y2), (*x3, *y3));
            }
            PathVerb::Close => {
                builder.close();
            }
            PathVerb::QuadTo { x1, y1, x2, y2 } => {
                builder.quad_to((*x1, *y1), (*x2, *y2));
            }
        };
    }
    builder.detach()
}

/// Converts a PDF Transform to a Skia Matrix.
fn to_skia_matrix(transform: &Transform) -> skia_safe::Matrix {
    skia_safe::Matrix::new_all(
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

fn to_skia_shader(shader: &Shader) -> Result<skia_safe::Shader, PdfCanvasError> {
    match shader {
        Shader::LinearGradient {
            x0,
            y0,
            x1,
            y1,
            transform,
            positions,
            colors,
        } => {
            let colors: Vec<skia_safe::Color> = colors
                .iter()
                .map(|color| skia_safe::Color4f::new(color.r, color.g, color.b, color.a).to_color())
                .collect();

            let mat = if let Some(m) = transform {
                to_skia_matrix(m)
            } else {
                skia_safe::Matrix::new_identity()
            };

            skia_safe::Shader::linear_gradient(
                (
                    skia_safe::Point::new(*x0, *y0),
                    skia_safe::Point::new(*x1, *y1),
                ),
                skia_safe::gradient_shader::GradientShaderColors::Colors(&colors),
                Some(positions.as_ref()),
                skia_safe::TileMode::Clamp,
                None,
                Some(&mat),
            )
            .ok_or_else(|| {
                SkiaCanvasBackendError::ShaderCreationFailed {
                    shader: "linear_gradient",
                }
                .into()
            })
        }
        Shader::RadialGradient {
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
            let colors: Vec<skia_safe::Color> = colors
                .iter()
                .map(|color| skia_safe::Color4f::new(color.r, color.g, color.b, color.a).to_color())
                .collect();

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
                Some(positions.as_ref()),
                skia_safe::TileMode::Clamp,
                None,
                Some(&mat),
            )
            .ok_or_else(|| {
                SkiaCanvasBackendError::ShaderCreationFailed {
                    shader: "two_point_conical_gradient",
                }
                .into()
            })
        }
        Shader::TilingPatternImage {
            image,
            transform,
            x_step: _,
            y_step: _,
        } => {
            let mat = if let Some(m) = transform {
                to_skia_matrix(m)
            } else {
                skia_safe::Matrix::new_identity()
            };

            let image = to_skia_a8_mask_image(image)?;
            image
                .to_shader(
                    (skia_safe::TileMode::Repeat, skia_safe::TileMode::Repeat),
                    skia_safe::SamplingOptions::default(),
                    Some(&mat),
                )
                .ok_or_else(|| {
                    SkiaCanvasBackendError::ShaderCreationFailed {
                        shader: "tiling_pattern_image",
                    }
                    .into()
                })
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

impl CanvasBackend for SkiaCanvasBackend<'_> {
    fn fill_path(
        &mut self,
        path: &PdfPath,
        fill_type: PathFillType,
        color: Color,
        shader: &Option<Shader>,
        blend_mode: Option<BlendMode>,
    ) -> Result<(), PdfCanvasError> {
        let mut sk_path = to_skia_path(path);
        sk_path.set_fill_type(to_skia_fill_type(fill_type));
        let mut paint = make_paint(color, skia_safe::paint::Style::Fill, None, blend_mode);
        if let Some(shader_spec) = shader {
            let shader = to_skia_shader(shader_spec)?;
            paint.set_shader(shader);
        }

        self.surface.canvas().draw_path(&sk_path, &paint);
        Ok(())
    }

    fn stroke_path(
        &mut self,
        path: &PdfPath,
        color: Color,
        line_width: f32,
        shader: &Option<Shader>,
        blend_mode: Option<BlendMode>,
    ) -> Result<(), PdfCanvasError> {
        let sk_path = to_skia_path(path);
        let mut paint = make_paint(
            color,
            skia_safe::paint::Style::Stroke,
            Some(line_width),
            blend_mode,
        );
        if let Some(shader_spec) = shader {
            let shader = to_skia_shader(shader_spec)?;
            paint.set_shader(shader);
        }

        self.surface.canvas().draw_path(&sk_path, &paint);
        Ok(())
    }

    fn width(&self) -> f32 {
        self.width
    }

    fn height(&self) -> f32 {
        self.height
    }

    fn set_clip_region(
        &mut self,
        path: &PdfPath,
        mode: PathFillType,
    ) -> Result<(), PdfCanvasError> {
        let mut sk_path = to_skia_path(path);
        sk_path.set_fill_type(to_skia_fill_type(mode));
        self.surface.canvas().clip_path(&sk_path, None, Some(true));
        Ok(())
    }

    fn save(&mut self) -> Result<(), PdfCanvasError> {
        self.surface.canvas().save();
        Ok(())
    }

    fn restore(&mut self) -> Result<(), PdfCanvasError> {
        self.surface.canvas().restore();
        Ok(())
    }

    fn draw_image_rect(
        &mut self,
        image: &Image<'_>,
        blend_mode: Option<BlendMode>,
        dest_rect: pdf_graphics::rect::Rect,
        image_rotation: Option<f32>,
    ) -> Result<(), PdfCanvasError> {
        if image.width == 0 || image.height == 0 {
            return Err(SkiaCanvasBackendError::InvalidImageDimensions {
                width: image.width,
                height: image.height,
            }
            .into());
        }

        let skia_image = to_skia_image(image)?;

        let mut paint = skia_safe::Paint::default();
        if let Some(mode) = blend_mode {
            paint.set_blend_mode(to_skia_blend_mode(mode));
        }

        let sk_rect = skia_safe::Rect::from_ltrb(
            dest_rect.left,
            dest_rect.top,
            dest_rect.right,
            dest_rect.bottom,
        );

        self.surface.canvas().save();
        if let Some(angle) = image_rotation {
            self.surface.canvas().rotate(
                angle,
                Some(skia_safe::Point {
                    x: sk_rect.center_x(),
                    y: sk_rect.center_y(),
                }),
            );
        }

        let sampling = skia_safe::SamplingOptions::new(
            skia_safe::FilterMode::Linear,
            skia_safe::MipmapMode::Nearest,
        );
        self.surface.canvas().draw_image_rect_with_sampling_options(
            &skia_image,
            None,
            sk_rect,
            sampling,
            &paint,
        );
        self.surface.canvas().restore();
        Ok(())
    }

    fn begin_mask_layer(
        &mut self,
        mask: &Arc<RecordingCanvas>,
        transform: &Transform,
        _mask_mode: MaskMode,
    ) -> Result<(), PdfCanvasError> {
        self.surface.canvas().save();
        let mat = to_skia_matrix(transform);
        let rect = skia_safe::Rect::from_xywh(0.0, 0.0, mask.width(), mask.height());
        let (rect, _) = mat.map_rect(rect);

        self.surface
            .canvas()
            .clip_rect(rect, skia_safe::ClipOp::Intersect, None);
        self.surface.canvas().clear(skia_safe::Color::WHITE);
        self.surface.canvas().save_layer(&Default::default());
        Ok(())
    }

    fn end_mask_layer(
        &mut self,
        mask: &Arc<RecordingCanvas>,
        transform: &Transform,
        mask_mode: MaskMode,
    ) -> Result<(), PdfCanvasError> {
        // Render mask into a temporary surface depending on the requested mask mode.
        // - Alpha: render directly into an A8 mask surface.
        // - Luminosity: render into RGBA and then convert RGB luminance into an A8 mask.
        let make_surface =
            |info: skia_safe::ImageInfo| -> Result<skia_safe::Surface, SkiaCanvasBackendError> {
                skia_safe::surfaces::raster(&info, None, None).ok_or(
                    SkiaCanvasBackendError::SurfaceAllocationFailed {
                        kind: "mask",
                        width: mask.width() as u32,
                        height: mask.height() as u32,
                    },
                )
            };

        // Create appropriate surface
        let mut surface = match mask_mode {
            MaskMode::Alpha => {
                let info =
                    skia_safe::ImageInfo::new_a8((mask.width() as i32, mask.height() as i32));
                make_surface(info)?
            }
            MaskMode::Luminosity => {
                // Use RGBA8888 Premul surface for rendering colored content
                let info = skia_safe::ImageInfo::new(
                    (mask.width() as i32, mask.height() as i32),
                    skia_safe::ColorType::RGBA8888,
                    skia_safe::AlphaType::Premul,
                    None,
                );
                make_surface(info)?
            }
        };

        // Replay the recorded mask drawing operations into the temporary surface.
        let mut mask_backend = SkiaCanvasBackend {
            surface: &mut surface,
            width: mask.width(),
            height: mask.height(),
        };
        mask.replay(&mut mask_backend)?;

        // Grab an image snapshot of the rendered mask content
        let mut mask_image = surface.image_snapshot();

        if mask_mode == MaskMode::Luminosity {
            // Convert RGBA to A8 using standard luminance coefficients.
            let w = mask_image.width();
            let h = mask_image.height();
            let rgba_info = skia_safe::ImageInfo::new(
                (w, h),
                skia_safe::ColorType::RGBA8888,
                skia_safe::AlphaType::Unpremul,
                None,
            );
            let row_bytes_rgba = (w as usize) * rgba_info.bytes_per_pixel();
            let mut rgba = vec![0u8; row_bytes_rgba * (h as usize)];
            let ok = mask_image.read_pixels(
                &rgba_info,
                rgba.as_mut_slice(),
                row_bytes_rgba,
                (0, 0),
                skia_safe::image::CachingHint::Allow,
            );
            if !ok {
                return Err(SkiaCanvasBackendError::ImageDecodeFailed {
                    encoding: "read_pixels",
                }
                .into());
            }
            // Number of bytes per pixel in RGBA format
            const BYTES_PER_RGBA: usize = 4;

            // Luminance coefficients per ITU-R BT.601 used to convert RGB to luma.
            const LUMA_COEFF_R_BT601: f32 = 0.299;
            const LUMA_COEFF_G_BT601: f32 = 0.587;
            const LUMA_COEFF_B_BT601: f32 = 0.114;

            // Compute luminance per pixel
            let mut a8 = vec![0u8; (w as usize) * (h as usize)];
            for (i, px) in rgba.chunks_exact(BYTES_PER_RGBA).enumerate() {
                let r = px[0] as f32;
                let g = px[1] as f32;
                let b = px[2] as f32;
                let y = LUMA_COEFF_R_BT601 * r + LUMA_COEFF_G_BT601 * g + LUMA_COEFF_B_BT601 * b;
                a8[i] = y.clamp(0.0, 255.0) as u8;
            }

            // Create an A8 image from the luminance buffer
            let a8_info = skia_safe::ImageInfo::new_a8((w, h));
            let row_bytes_a8 = w as usize;
            if let Some(img) = skia_safe::images::raster_from_data(
                &a8_info,
                skia_safe::Data::new_copy(&a8),
                row_bytes_a8,
            ) {
                mask_image = img;
            }
        }

        // If PDF transform applies to the mask coordinate system, apply only for drawing mask.
        let mat = skia_safe::M44::from(to_skia_matrix(transform));
        self.surface.canvas().set_matrix(&mat);

        // Apply mask: multiply destination alpha by mask alpha
        let mut paint = skia_safe::Paint::default();
        paint.set_blend_mode(skia_safe::BlendMode::DstIn);

        // Skia's coordinate system has the origin at the top-left, so we need to flip the mask vertically.
        self.surface
            .canvas()
            .draw_image(mask_image, (0.0, 0.0), Some(&paint));

        // Pop the layer (masked content merges down).
        self.surface.canvas().restore();
        self.surface.canvas().restore();
        Ok(())
    }
}
