use std::borrow::Cow;

use pdf_canvas::{
    canvas_backend::{CanvasBackend, Image, Shader},
    recording_canvas::RecordingCanvas,
};
use pdf_graphics::{
    BlendMode, ImageEncoding, MaskMode, PathFillType,
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
    #[error("unsupported number of image components: {components} for image {width}x{height}")]
    UnsupportedImageComponents {
        components: usize,
        width: usize,
        height: usize,
    },
    #[error("invalid image dimensions: {width}x{height}")]
    InvalidImageDimensions { width: u32, height: u32 },
    #[error("failed to decode image with encoding: {encoding}")]
    ImageDecodeFailed { encoding: &'static str },
    #[error("failed to create skia raster image from data ({width}x{height})")]
    RasterImageCreationFailed { width: u32, height: u32 },
    #[error("failed to create shader: {shader}")]
    ShaderCreationFailed { shader: &'static str },
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
) -> Result<skia_safe::Image, SkiaCanvasBackendError> {
    let info = skia_safe::ImageInfo::new_a8((
        recording_canvas.width() as i32,
        recording_canvas.height() as i32,
    ));
    let Some(mut surface) = skia_safe::surfaces::raster(&info, None, None) else {
        return Err(SkiaCanvasBackendError::SurfaceAllocationFailed {
            kind: "mask",
            width: recording_canvas.width() as u32,
            height: recording_canvas.height() as u32,
        });
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
/// Supports JPEG-encoded data via Skia's decoder and raw pixel buffers with
/// 1 (A8), 3 (RGB), or 4 (RGBA) components. When a soft mask is present, the
/// alpha channel is combined with the mask to produce premultiplied output as
/// needed. The resulting Skia image uses `AlphaType::Unpremul`.
///
/// # Parameters
///
/// - `image`: PDF image descriptor containing pixel data, dimensions, optional
///   encoding (e.g., "jpeg"), transform, and optional soft mask.
///
/// # Returns
///
/// - A Skia `Image` ready to be drawn with `draw_image`/`draw_image_rect`.
fn to_skia_image(image: &Image<'_>) -> Result<skia_safe::Image, SkiaCanvasBackendError> {
    if image.encoding == ImageEncoding::Jpeg {
        return skia_safe::Image::from_encoded(skia_safe::Data::new_copy(&image.data))
            .ok_or(SkiaCanvasBackendError::ImageDecodeFailed { encoding: "jpeg" });
    }

    let (color_type, pixel_data) = get_skia_image_data(
        &image.data,
        image.width as usize,
        image.height as usize,
        &image.mask,
    )?;

    let image_info = skia_safe::ImageInfo::new(
        (image.width as i32, image.height as i32),
        color_type,
        skia_safe::AlphaType::Unpremul,
        None,
    );

    let row_bytes = image.width as usize * image_info.bytes_per_pixel();
    skia_safe::images::raster_from_data(&image_info, pixel_data, row_bytes).ok_or(
        SkiaCanvasBackendError::RasterImageCreationFailed {
            width: image.width,
            height: image.height,
        },
    )
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

fn to_skia_shader(shader: &Shader) -> Result<skia_safe::Shader, SkiaCanvasBackendError> {
    match shader {
        &Shader::LinearGradient {
            x0,
            y0,
            x1,
            y1,
            positions,
            colors,
        } => {
            let colors: Vec<skia_safe::Color> = colors
                .iter()
                .map(|color| skia_safe::Color4f::new(color.r, color.g, color.b, color.a).to_color())
                .collect();

            skia_safe::Shader::linear_gradient(
                (skia_safe::Point::new(x0, y0), skia_safe::Point::new(x1, y1)),
                skia_safe::gradient_shader::GradientShaderColors::Colors(&colors),
                Some(positions),
                skia_safe::TileMode::Clamp,
                None,
                None,
            )
            .ok_or(SkiaCanvasBackendError::ShaderCreationFailed {
                shader: "linear_gradient",
            })
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
            .ok_or(SkiaCanvasBackendError::ShaderCreationFailed {
                shader: "two_point_conical_gradient",
            })
        }
        Shader::TilingPatternImage {
            image,
            transform: _,
            x_step: _,
            y_step: _,
        } => {
            let image = to_skia_a8_mask_image(image)?;
            image
                .to_shader(
                    (skia_safe::TileMode::Repeat, skia_safe::TileMode::Repeat),
                    skia_safe::SamplingOptions::default(),
                    None,
                )
                .ok_or(SkiaCanvasBackendError::ShaderCreationFailed {
                    shader: "tiling_pattern_image",
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

/// Converts image data to Skia's expected format.
fn get_skia_image_data(
    image: &[u8],
    width: usize,
    height: usize,
    smask: &Option<Cow<'_, [u8]>>,
) -> Result<(skia_safe::ColorType, skia_safe::Data), SkiaCanvasBackendError> {
    let num_pixels = width * height;
    let num_components = image.len() / num_pixels;
    match num_components {
        // RGBA input. If no soft mask, we can pass-through without copying.
        4 if smask.is_none() => Ok((
            skia_safe::ColorType::RGBA8888,
            skia_safe::Data::new_copy(image),
        )),
        4 => {
            let mut out = Vec::with_capacity(image.len());
            let smask_bytes = smask.as_ref().map(|s| s.as_ref());
            for (i, rgba) in image.chunks_exact(4).enumerate() {
                let sm = smask_bytes.and_then(|s| s.get(i)).copied().unwrap_or(255);
                let new_a = (u16::from(rgba[3]) * u16::from(sm) / 255) as u8;
                out.extend_from_slice(&[rgba[0], rgba[1], rgba[2], new_a]);
            }
            Ok((
                skia_safe::ColorType::RGBA8888,
                skia_safe::Data::new_copy(&out),
            ))
        }
        // RGB input. Expand to RGBA, using soft mask alpha when present.
        3 => {
            let mut out = Vec::with_capacity(num_pixels * 4);
            let smask_bytes = smask.as_ref().map(|s| s.as_ref());
            for (i, rgb) in image.chunks_exact(3).enumerate() {
                let a = smask_bytes.and_then(|s| s.get(i)).copied().unwrap_or(255);
                out.extend_from_slice(&[rgb[0], rgb[1], rgb[2], a]);
            }
            Ok((
                skia_safe::ColorType::RGBA8888,
                skia_safe::Data::new_copy(&out),
            ))
        }
        // Grayscale mask (A8)
        1 => Ok((
            skia_safe::ColorType::Alpha8,
            skia_safe::Data::new_copy(image),
        )),
        _ => Err(SkiaCanvasBackendError::UnsupportedImageComponents {
            components: num_components,
            width,
            height,
        }),
    }
}

impl CanvasBackend for SkiaCanvasBackend<'_> {
    type ErrorType = SkiaCanvasBackendError;

    fn fill_path(
        &mut self,
        path: &PdfPath,
        fill_type: PathFillType,
        color: Color,
        shader: &Option<Shader>,
        blend_mode: Option<BlendMode>,
    ) -> Result<(), Self::ErrorType> {
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
    ) -> Result<(), Self::ErrorType> {
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
    ) -> Result<(), Self::ErrorType> {
        let mut sk_path = to_skia_path(path);
        sk_path.set_fill_type(to_skia_fill_type(mode));
        self.surface.canvas().save();
        self.surface
            .canvas()
            .clip_path(&sk_path, skia_safe::ClipOp::Intersect, Some(true));
        Ok(())
    }

    fn reset_clip(&mut self) -> Result<(), Self::ErrorType> {
        self.surface.canvas().restore();
        Ok(())
    }

    fn draw_image(
        &mut self,
        image: &Image<'_>,
        blend_mode: Option<BlendMode>,
    ) -> Result<(), Self::ErrorType> {
        if image.width == 0 || image.height == 0 {
            return Err(SkiaCanvasBackendError::InvalidImageDimensions {
                width: image.width,
                height: image.height,
            });
        }

        let skia_image = to_skia_image(image)?;

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
        Ok(())
    }

    fn begin_mask_layer(
        &mut self,
        mask: &RecordingCanvas,
        transform: &Transform,
        _mask_mode: MaskMode,
    ) -> Result<(), Self::ErrorType> {
        self.surface.canvas().save();
        let mat = to_skia_matrix(transform);
        let rect = skia_safe::Rect::from_xywh(0.0, -mask.height(), mask.width(), mask.height());
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
        mask: &RecordingCanvas,
        transform: &Transform,
        mask_mode: MaskMode,
    ) -> Result<(), Self::ErrorType> {
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

        // If PDF transform applies to the mask coordinate system, apply only for drawing mask.
        let mat = to_skia_matrix(transform);
        self.surface.canvas().concat(&mat);

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
                });
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

        // Apply mask: multiply destination alpha by mask alpha
        let mut paint = skia_safe::Paint::default();
        paint.set_blend_mode(skia_safe::BlendMode::DstIn);

        // Skia's coordinate system has the origin at the top-left, so we need to flip the mask vertically.
        let height = -mask_image.height() as f32;
        self.surface
            .canvas()
            .draw_image(mask_image, (0.0, height), Some(&paint));

        // Pop the layer (masked content merges down).
        self.surface.canvas().restore();
        self.surface.canvas().restore();
        Ok(())
    }
}
