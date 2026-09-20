//! Skia rendering using native state and locally scoped mask surfaces.

use pdf_canvas::{
    CanvasPath,
    canvas_backend::{CanvasBackend, Shader},
    error::PdfCanvasError,
    mask_layer::{CoverageTarget, MaskLayer},
    stroke_style::{StrokeStyle, device_stroke_width},
    tiling_shader::TilingShader,
    viewport::pixel_extent,
};
use pdf_graphics::{
    BlendMode, Image, PathFillType, PixelFormat, color::Color, pdf_path::PathVerb,
    transform::Transform,
};
use pdf_shading::paint::ShadingPaint;

/// Native allocation, conversion, and mask replay failures.
#[derive(Debug, thiserror::Error)]
pub enum SkiaCanvasBackendError {
    /// Logical mask dimensions cannot be represented by Skia.
    #[error("mask dimensions exceed Skia limits: {width} x {height}")]
    MaskDimensions {
        /// Requested width, retained for diagnostics.
        width: f32,
        /// Requested height, retained for diagnostics.
        height: f32,
    },
    /// A checked native size conversion failed.
    #[error(transparent)]
    Integer(#[from] std::num::TryFromIntError),
    /// A pixel buffer could not be allocated.
    #[error(transparent)]
    Allocation(#[from] std::collections::TryReserveError),
    /// Portable pixel preparation failed.
    #[error(transparent)]
    Raster(#[from] pdf_image::ImageRasterError),
    /// Replaying mask content failed.
    #[error(transparent)]
    Canvas(#[from] PdfCanvasError),
    #[error("failed to allocate raster surface for {kind} ({width}x{height})")]
    /// A native raster surface could not be allocated.
    SurfaceAllocationFailed {
        /// Purpose of the requested surface.
        kind: &'static str,
        /// Requested pixel width.
        width: u32,
        /// Requested pixel height.
        height: u32,
    },
    #[error("invalid image dimensions: {width}x{height}")]
    /// Image dimensions are empty or invalid.
    InvalidImageDimensions {
        /// Requested width, retained for diagnostics.
        width: usize,
        /// Requested height, retained for diagnostics.
        height: usize,
    },
    #[error("failed to decode image with encoding: {encoding}")]
    /// Image decoding or native pixel readback failed.
    ImageDecodeFailed {
        /// Decoder or readback operation that failed.
        encoding: &'static str,
    },
    #[error("failed to create skia raster image from data ({width}x{height})")]
    /// Native image creation failed for the prepared pixels.
    RasterImageCreationFailed {
        /// Requested width, retained for diagnostics.
        width: usize,
        /// Requested height, retained for diagnostics.
        height: usize,
    },
    #[error("failed to create shader: {shader}")]
    /// A native shader could not be constructed.
    ShaderCreationFailed {
        /// Shader kind that could not be prepared.
        shader: &'static str,
    },
    #[error("failed to create dash path effect")]
    /// A dash effect could not be constructed.
    DashPathEffectCreationFailed,
    #[error("stroke width is not finite")]
    /// The requested stroke width cannot be drawn.
    InvalidStrokeWidth,
}

impl From<SkiaCanvasBackendError> for PdfCanvasError {
    /// Converts this error at the canvas boundary while retaining its diagnostic.
    fn from(e: SkiaCanvasBackendError) -> Self {
        match e {
            SkiaCanvasBackendError::Canvas(error) => error,
            other => PdfCanvasError::BackendError(other.to_string()),
        }
    }
}

/// Borrows a Skia surface; native canvas state remains the sole rendering-state stack.
pub struct SkiaCanvasBackend<'a> {
    /// Native rendering target owned by the caller.
    pub surface: &'a mut skia_safe::Surface,
    /// Logical device width.
    pub width: f32,
    /// Logical device height.
    pub height: f32,
}

/// Renders the cells intersecting one prepared repeat period into a Skia image.
///
/// The period is sampled at the backing density implied by `device_to_backing` and
/// returned with the local matrix that maps its pixels back into device space.
fn to_skia_tiling_image(
    pattern: &TilingShader,
    device_to_backing: &Transform,
) -> Result<(skia_safe::Image, skia_safe::Matrix), PdfCanvasError> {
    let plan = pattern.raster_plan(device_to_backing)?;
    let [width, height] = plan.size;
    let info = skia_safe::ImageInfo::new(
        (
            i32::try_from(width).map_err(SkiaCanvasBackendError::from)?,
            i32::try_from(height).map_err(SkiaCanvasBackendError::from)?,
        ),
        skia_safe::ColorType::RGBA8888,
        skia_safe::AlphaType::Unpremul,
        None,
    );
    let Some(mut surface) = skia_safe::surfaces::raster(&info, None, None) else {
        return Err(SkiaCanvasBackendError::SurfaceAllocationFailed {
            kind: "pattern",
            width,
            height,
        }
        .into());
    };

    let [step_x, step_y] = pattern.repeat_step();
    let mut tile_backend = SkiaCanvasBackend {
        surface: &mut surface,
        width: step_x,
        height: step_y,
    };
    for transform in pattern.cell_transforms() {
        tile_backend.surface.canvas().save();
        tile_backend.surface.canvas().concat(&to_skia_matrix(
            &plan.cell_to_pixel.post_concatenated(transform),
        ));
        let result = pattern.recording().replay(&mut tile_backend);
        tile_backend.surface.canvas().restore();
        result?;
    }
    let local = pattern
        .pattern_transform()
        .post_concatenated(&plan.cell_to_pixel.try_inverse()?);
    Ok((surface.image_snapshot(), to_skia_matrix(&local)))
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
fn to_skia_image(image: &Image) -> Result<skia_safe::Image, PdfCanvasError> {
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

/// Resolves device-coordinate geometry into a Skia path.
fn to_skia_path(pdf_path: &CanvasPath<'_>) -> Result<skia_safe::Path, PdfCanvasError> {
    let mut builder = skia_safe::PathBuilder::new();
    for verb in pdf_path.verbs() {
        match &verb? {
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
    Ok(builder.detach())
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

/// Converts a Skia matrix into a PDF transform, dropping any perspective components.
fn from_skia_matrix(matrix: &skia_safe::Matrix) -> Transform {
    Transform::from_row(
        matrix.scale_x(),
        matrix.skew_y(),
        matrix.skew_x(),
        matrix.scale_y(),
        matrix.translate_x(),
        matrix.translate_y(),
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
        BlendMode::Unknown(_) => skia_safe::BlendMode::SrcOver,
    }
}

/// Creates a native shader from shared shading or tiling-pattern data.
///
/// `device_to_backing` is the canvas's current local-to-device matrix, used to
/// rasterize tiling periods at the density they are painted with.
fn to_skia_shader(
    shader: &Shader,
    device_to_backing: &Transform,
) -> Result<skia_safe::Shader, PdfCanvasError> {
    match shader {
        Shader::Shading(ShadingPaint::LinearGradient {
            x0,
            y0,
            x1,
            y1,
            transform,
            positions,
            colors,
        }) => {
            let colors: Vec<skia_safe::Color> = colors
                .iter()
                .map(|color| skia_safe::Color4f::new(color.r, color.g, color.b, color.a).to_color())
                .collect();

            let mat = to_skia_matrix(transform);

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
        Shader::Shading(ShadingPaint::RadialGradient {
            start_x,
            start_y,
            start_r,
            end_x,
            end_y,
            end_r,
            positions,
            colors,
            transform,
        }) => {
            let colors: Vec<skia_safe::Color> = colors
                .iter()
                .map(|color| skia_safe::Color4f::new(color.r, color.g, color.b, color.a).to_color())
                .collect();

            let mat = to_skia_matrix(transform);

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
        Shader::TilingPatternImage(pattern) => {
            let (image, mat) = to_skia_tiling_image(pattern, device_to_backing)?;
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
        Shader::Shading(ShadingPaint::RasterImage { image, transform }) => {
            let image = to_skia_image(image)?;
            let matrix = to_skia_matrix(transform);
            image
                .to_shader(
                    (skia_safe::TileMode::Decal, skia_safe::TileMode::Decal),
                    skia_safe::SamplingOptions::default(),
                    Some(&matrix),
                )
                .ok_or_else(|| {
                    SkiaCanvasBackendError::ShaderCreationFailed {
                        shader: "raster_image",
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
    /// Fills the supplied device-space path using the requested paint and fill rule.
    fn fill_path(
        &mut self,
        path: &CanvasPath<'_>,
        fill_type: PathFillType,
        color: Color,
        shader: Option<&Shader>,
        blend_mode: Option<BlendMode>,
    ) -> Result<(), PdfCanvasError> {
        let mut sk_path = to_skia_path(path)?;
        sk_path.set_fill_type(to_skia_fill_type(fill_type));
        let mut paint = make_paint(color, skia_safe::paint::Style::Fill, None, blend_mode);
        if let Some(shader_spec) = shader {
            let mapping = from_skia_matrix(&self.surface.canvas().local_to_device_as_3x3());
            paint.set_shader(to_skia_shader(shader_spec, &mapping)?);
        }

        self.surface.canvas().draw_path(&sk_path, &paint);
        Ok(())
    }

    /// Strokes device-space geometry using the supplied width and stroke style.
    fn stroke_path(
        &mut self,
        path: &CanvasPath<'_>,
        color: Color,
        line_width: f32,
        stroke_style: &StrokeStyle,
        shader: Option<&Shader>,
        blend_mode: Option<BlendMode>,
    ) -> Result<(), PdfCanvasError> {
        let sk_path = to_skia_path(path)?;
        // Skia draws zero-width strokes as hairlines.
        let line_width = device_stroke_width(line_width, 0.0)
            .ok_or(SkiaCanvasBackendError::InvalidStrokeWidth)?;
        let mut paint = make_paint(
            color,
            skia_safe::paint::Style::Stroke,
            Some(line_width),
            blend_mode,
        );
        paint.set_stroke_cap(match stroke_style.line_cap {
            pdf_graphics::LineCap::Butt => skia_safe::paint::Cap::Butt,
            pdf_graphics::LineCap::Round => skia_safe::paint::Cap::Round,
            pdf_graphics::LineCap::Square => skia_safe::paint::Cap::Square,
        });
        paint.set_stroke_join(match stroke_style.line_join {
            pdf_graphics::LineJoin::Miter => skia_safe::paint::Join::Miter,
            pdf_graphics::LineJoin::Round => skia_safe::paint::Join::Round,
            pdf_graphics::LineJoin::Bevel => skia_safe::paint::Join::Bevel,
        });
        paint.set_stroke_miter(stroke_style.miter_limit);
        if let Some(dash_pattern) = &stroke_style.dash_pattern {
            let effect = skia_safe::PathEffect::dash(&dash_pattern.intervals, dash_pattern.phase)
                .ok_or(SkiaCanvasBackendError::DashPathEffectCreationFailed)?;
            paint.set_path_effect(effect);
        }
        if let Some(shader_spec) = shader {
            let mapping = from_skia_matrix(&self.surface.canvas().local_to_device_as_3x3());
            paint.set_shader(to_skia_shader(shader_spec, &mapping)?);
        }

        self.surface.canvas().draw_path(&sk_path, &paint);
        Ok(())
    }

    /// Returns the logical canvas width used by PDF painting.
    fn width(&self) -> f32 {
        self.width
    }

    /// Returns the logical canvas height used by PDF painting.
    fn height(&self) -> f32 {
        self.height
    }

    /// Intersects subsequent painting with the supplied device-space clipping path.
    fn set_clip_region(
        &mut self,
        path: &CanvasPath<'_>,
        mode: PathFillType,
    ) -> Result<(), PdfCanvasError> {
        let mut sk_path = to_skia_path(path)?;
        sk_path.set_fill_type(to_skia_fill_type(mode));
        self.surface.canvas().clip_path(&sk_path, None, Some(true));
        Ok(())
    }

    /// Saves the current clipping and drawing state for a matching restore.
    fn save(&mut self) -> Result<(), PdfCanvasError> {
        self.surface.canvas().save();
        Ok(())
    }

    /// Restores the most recently saved drawing state.
    fn restore(&mut self) -> Result<(), PdfCanvasError> {
        self.surface.canvas().restore();
        Ok(())
    }

    /// Draws decoded image pixels into the supplied destination rectangle.
    fn draw_image_rect(
        &mut self,
        image: &Image,
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

    /// Keeps native layer state local and restores it explicitly after the callback returns.
    fn with_mask_layer<F>(
        &mut self,
        mask: &MaskLayer,
        paint_content: F,
    ) -> Result<(), PdfCanvasError>
    where
        F: FnOnce(&mut Self) -> Result<(), PdfCanvasError>,
    {
        if mask.is_passthrough() {
            return paint_content(self);
        }
        let coverage = mask.render_coverage(&mut MaskRaster)?;
        let canvas = self.surface.canvas();
        let parent = canvas.save_count();
        canvas.save();
        let mapping = canvas.local_to_device();
        canvas.concat(&to_skia_matrix(mask.transform()));
        canvas.clip_rect(
            skia_safe::Rect::from_xywh(
                0.0,
                0.0,
                mask.recording().width(),
                mask.recording().height(),
            ),
            skia_safe::ClipOp::Intersect,
            None,
        );
        canvas.set_matrix(&mapping);
        canvas.save_layer(&Default::default());
        let layer = canvas.save_count();
        // Protect the initial clip so failed painting can be discarded in its entirety.
        canvas.save();
        let result = paint_content(self);
        let canvas = self.surface.canvas();
        canvas.restore_to_count(layer);
        if result.is_ok() {
            canvas.concat(&to_skia_matrix(mask.transform()));
            let mut paint = skia_safe::Paint::default();
            paint.set_blend_mode(skia_safe::BlendMode::DstIn);
            canvas.draw_image(coverage, (0.0, 0.0), Some(&paint));
        } else {
            canvas.clear(skia_safe::Color::TRANSPARENT);
        }
        canvas.restore_to_count(parent);
        result
    }
}

/// Rasterizes mask coverage in mask space; the mask transform is applied when compositing.
struct MaskRaster;

/// Resolves the mask recording's logical dimensions into native pixel dimensions.
fn mask_dimensions(mask: &MaskLayer) -> Result<(i32, i32), SkiaCanvasBackendError> {
    let extent = |value: f32| {
        pixel_extent(value)
            .and_then(|extent| i32::try_from(extent).ok())
            .ok_or(SkiaCanvasBackendError::MaskDimensions {
                width: mask.recording().width(),
                height: mask.recording().height(),
            })
    };
    Ok((
        extent(mask.recording().width())?,
        extent(mask.recording().height())?,
    ))
}

impl CoverageTarget for MaskRaster {
    type Raster = skia_safe::Image;

    /// Renders native coverage before any content layer is opened on the destination.
    fn replay_mask(&mut self, mask: &MaskLayer) -> Result<Self::Raster, PdfCanvasError> {
        let (width, height) = mask_dimensions(mask)?;
        let info = if mask.needs_processing() {
            skia_safe::ImageInfo::new(
                (width, height),
                skia_safe::ColorType::RGBA8888,
                skia_safe::AlphaType::Premul,
                None,
            )
        } else {
            skia_safe::ImageInfo::new_a8((width, height))
        };
        let mut surface = skia_safe::surfaces::raster(&info, None, None).ok_or(
            SkiaCanvasBackendError::SurfaceAllocationFailed {
                kind: "mask",
                width: u32::try_from(width).map_err(SkiaCanvasBackendError::from)?,
                height: u32::try_from(height).map_err(SkiaCanvasBackendError::from)?,
            },
        )?;
        let mut backend = SkiaCanvasBackend {
            surface: &mut surface,
            width: mask.recording().width(),
            height: mask.recording().height(),
        };
        mask.recording().replay(&mut backend)?;
        Ok(surface.image_snapshot())
    }

    fn read(&mut self, raster: Self::Raster) -> Result<Image, PdfCanvasError> {
        let (width, height) = (raster.width(), raster.height());
        let (w, h) = (
            usize::try_from(width).map_err(SkiaCanvasBackendError::from)?,
            usize::try_from(height).map_err(SkiaCanvasBackendError::from)?,
        );
        let size = w
            .checked_mul(h)
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or(pdf_image::ImageRasterError::ResourceLimit)?;
        let rgba_info = skia_safe::ImageInfo::new(
            (width, height),
            skia_safe::ColorType::RGBA8888,
            skia_safe::AlphaType::Unpremul,
            None,
        );
        let mut pixels = Vec::new();
        pixels.try_reserve_exact(size)?;
        pixels.resize(size, 0);
        if !raster.read_pixels(
            &rgba_info,
            &mut pixels,
            rgba_info.min_row_bytes(),
            (0, 0),
            skia_safe::image::CachingHint::Allow,
        ) {
            return Err(SkiaCanvasBackendError::ImageDecodeFailed {
                encoding: "mask read_pixels",
            }
            .into());
        }
        Ok(Image {
            data: pixels.into(),
            width: w,
            height: h,
            pixel_format: PixelFormat::RGBA8888,
        })
    }

    /// Keeps only the coverage alpha, which is all the destination-in composite samples.
    fn upload(&mut self, image: &Image) -> Result<Self::Raster, PdfCanvasError> {
        let (width, height) = (
            i32::try_from(image.width).map_err(SkiaCanvasBackendError::from)?,
            i32::try_from(image.height).map_err(SkiaCanvasBackendError::from)?,
        );
        let mut alpha = Vec::new();
        alpha.try_reserve_exact(image.data.len() / 4)?;
        alpha.extend(
            image
                .data
                .as_chunks::<4>()
                .0
                .iter()
                .map(|&[_, _, _, alpha]| alpha),
        );
        skia_safe::images::raster_from_data(
            &skia_safe::ImageInfo::new_a8((width, height)),
            skia_safe::Data::new_copy(&alpha),
            image.width,
        )
        .ok_or_else(|| {
            SkiaCanvasBackendError::RasterImageCreationFailed {
                width: image.width,
                height: image.height,
            }
            .into()
        })
    }
}

#[cfg(test)]
mod tests {
    use pdf_canvas::CanvasPath;
    use pdf_graphics::BlendMode;

    use super::to_skia_blend_mode;

    /// Produces solid mask coverage with independently supplied logical dimensions.
    fn mask(width: f32, height: f32) -> pdf_canvas::mask_layer::MaskLayer {
        use pdf_canvas::{
            canvas_backend::CanvasBackend, mask_layer::MaskLayer, recording_canvas::RecordingCanvas,
        };
        use pdf_graphics::{
            MaskMode, PathFillType, color::Color, pdf_path::PdfPath, rect::Rect,
            transform::Transform,
        };
        let mut recording = RecordingCanvas::new(width, height);
        recording
            .fill_path(
                &CanvasPath::device(&PdfPath::from(&Rect::new(width, height))),
                PathFillType::Winding,
                Color::from_rgb(1.0, 1.0, 1.0),
                None,
                None,
            )
            .unwrap();
        MaskLayer::new(
            std::sync::Arc::new(recording),
            Transform::identity(),
            MaskMode::Alpha,
            None,
        )
        .unwrap()
    }
    /// Reads a native pixel without relying on screenshot comparison tolerances.
    fn pixel(surface: &mut skia_safe::Surface, x: i32, y: i32) -> [u8; 4] {
        let info = skia_safe::ImageInfo::new(
            (1, 1),
            skia_safe::ColorType::RGBA8888,
            skia_safe::AlphaType::Unpremul,
            None,
        );
        let mut bytes = [0; 4];
        assert!(surface.image_snapshot().read_pixels(
            &info,
            &mut bytes,
            4,
            (x, y),
            skia_safe::image::CachingHint::Disallow
        ));
        bytes
    }
    /// Failure discards all content despite a narrowed clip, preserving the native parent count.
    #[test]
    fn callback_failure_restores_original_clip_and_native_state() {
        use pdf_canvas::{canvas_backend::CanvasBackend, error::PdfCanvasError};
        use pdf_graphics::{PathFillType, color::Color, pdf_path::PdfPath, rect::Rect};
        let mut surface = skia_safe::surfaces::raster_n32_premul((16, 16)).unwrap();
        surface.canvas().clear(skia_safe::Color::WHITE);
        let count = surface.canvas().save_count();
        let mut backend = super::SkiaCanvasBackend {
            surface: &mut surface,
            width: 16.0,
            height: 16.0,
        };
        let result = backend.with_mask_layer(&mask(16.0, 16.0), |backend| {
            backend.fill_path(
                &CanvasPath::device(&PdfPath::from(&Rect::new(16.0, 16.0))),
                PathFillType::Winding,
                Color::from_rgb(1.0, 0.0, 0.0),
                None,
                None,
            )?;
            backend.set_clip_region(
                &CanvasPath::device(&PdfPath::from(&Rect::new(2.0, 2.0))),
                PathFillType::Winding,
            )?;
            Err(PdfCanvasError::CurrentPointRequired)
        });
        assert!(matches!(result, Err(PdfCanvasError::CurrentPointRequired)));
        assert_eq!(backend.surface.canvas().save_count(), count);
        assert_eq!(pixel(backend.surface, 1, 1), [255, 255, 255, 255]);
        assert_eq!(pixel(backend.surface, 12, 12), [255, 255, 255, 255]);
        backend
            .with_mask_layer(&mask(16.0, 16.0), |backend| {
                backend.fill_path(
                    &CanvasPath::device(&PdfPath::from(&Rect::new(16.0, 16.0))),
                    PathFillType::Winding,
                    Color::from_rgb(0.0, 0.0, 1.0),
                    None,
                    None,
                )
            })
            .unwrap();
        assert_eq!(pixel(backend.surface, 12, 12), [0, 0, 255, 255]);
        assert_eq!(backend.surface.canvas().save_count(), count);
    }
    /// Oversized native raster dimensions fail before painting or changing the destination.
    #[test]
    fn preparation_failure_skips_callback() {
        use pdf_canvas::canvas_backend::CanvasBackend;
        let mut surface = skia_safe::surfaces::raster_n32_premul((16, 16)).unwrap();
        let count = surface.canvas().save_count();
        let mut backend = super::SkiaCanvasBackend {
            surface: &mut surface,
            width: 16.0,
            height: 16.0,
        };
        let mut called = false;
        assert!(
            backend
                .with_mask_layer(&mask(f32::MAX, 1.0), |_| {
                    called = true;
                    Ok(())
                })
                .is_err()
        );
        assert!(!called);
        assert_eq!(backend.surface.canvas().save_count(), count);
    }

    #[test]
    /// Verifies that unknown blend mode uses normal compositing.
    fn unknown_blend_mode_uses_normal_compositing() {
        assert_eq!(
            to_skia_blend_mode(BlendMode::Unknown(Vec::from(b"VendorBlend"))),
            skia_safe::BlendMode::SrcOver
        );
    }
}
