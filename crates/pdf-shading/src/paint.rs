//! Backend-facing shading paint descriptions and builders.

use std::sync::Arc;

use bytes::Bytes;
use num_traits::ToPrimitive;
use pdf_graphics::{Image, PixelFormat, color::Color, rect::Rect, transform::Transform};

use crate::{
    color_stops::validate_stops,
    error::{PdfShadingError, ShadingRasterError},
    mesh::{
        MeshPatchRef, patch_mesh_bounds, rasterize_mesh_patches, rasterize_mesh_triangles,
        triangle_mesh_bounds,
    },
    model::Shading,
};

/// A backend-ready representation of a parsed PDF shading.
///
/// Equality compares field values, including the contents of shared buffers.
/// Floating-point fields use ordinary equality, so paints containing NaN may
/// compare unequal to themselves and this type does not implement `Eq`.
#[derive(Clone, PartialEq)]
pub enum ShadingPaint {
    /// A linear gradient shading paint.
    LinearGradient {
        /// Gradient start point x-coordinate.
        x0: f32,
        /// Gradient start point y-coordinate.
        y0: f32,
        /// Gradient end point x-coordinate.
        x1: f32,
        /// Gradient end point y-coordinate.
        y1: f32,
        /// Transform mapping shading space into device space.
        transform: Transform,
        /// Gradient colors.
        colors: Arc<[Color]>,
        /// Gradient stop positions.
        positions: Arc<[f32]>,
    },
    /// A radial gradient shading paint.
    RadialGradient {
        /// Start circle center x-coordinate.
        start_x: f32,
        /// Start circle center y-coordinate.
        start_y: f32,
        /// Start circle radius.
        start_r: f32,
        /// End circle center x-coordinate.
        end_x: f32,
        /// End circle center y-coordinate.
        end_y: f32,
        /// End circle radius.
        end_r: f32,
        /// Gradient colors.
        colors: Arc<[Color]>,
        /// Gradient stop positions.
        positions: Arc<[f32]>,
        /// Transform mapping shading space into device space.
        transform: Transform,
    },
    /// A rasterized mesh shading paint.
    RasterImage {
        /// Render-ready RGBA8 source image.
        image: Image,
        /// Transform mapping source pixel coordinates into device space.
        transform: Transform,
    },
}

impl ShadingPaint {
    /// Builds a validated linear-gradient paint with normalized colors.
    pub fn linear_gradient(
        coords: [f32; 4],
        transform: Option<Transform>,
        positions: Arc<[f32]>,
        colors: Arc<[Color]>,
    ) -> Result<Self, ShadingRasterError> {
        let [x0, y0, x1, y1] = coords;
        let paint = Self::LinearGradient {
            x0,
            y0,
            x1,
            y1,
            transform: prepare_transform(transform)?,
            positions,
            colors: normalized_colors(&colors)?,
        };
        paint.validate()?;
        Ok(paint)
    }

    /// Builds a validated radial-gradient paint with normalized colors.
    pub fn radial_gradient(
        coords: [f32; 6],
        transform: Option<Transform>,
        positions: Arc<[f32]>,
        colors: Arc<[Color]>,
    ) -> Result<Self, ShadingRasterError> {
        let [start_x, start_y, start_r, end_x, end_y, end_r] = coords;
        let paint = Self::RadialGradient {
            start_x,
            start_y,
            start_r,
            end_x,
            end_y,
            end_r,
            positions,
            colors: normalized_colors(&colors)?,
            transform: prepare_transform(transform)?,
        };
        paint.validate()?;
        Ok(paint)
    }

    /// Builds a validated raster paint and folds destination placement into its transform.
    pub fn raster_image(
        image: Image,
        dest_rect: Rect,
        transform: Option<Transform>,
    ) -> Result<Self, ShadingRasterError> {
        if image.width == 0 || image.height == 0 {
            return Err(ShadingRasterError::InvalidInput("empty image"));
        }
        if image.pixel_format != PixelFormat::RGBA8888
            || image.data.len()
                != image
                    .rgba_byte_size()
                    .ok_or(ShadingRasterError::ResourceLimit)?
            || !dest_rect.is_valid()
        {
            return Err(ShadingRasterError::InvalidInput("raster image"));
        }
        let image_width = image
            .width
            .to_f32()
            .ok_or(ShadingRasterError::ResourceLimit)?;
        let image_height = image
            .height
            .to_f32()
            .ok_or(ShadingRasterError::ResourceLimit)?;
        let placement = Transform::from_row(
            dest_rect.width().max(1.0) / image_width,
            0.0,
            0.0,
            dest_rect.height().max(1.0) / image_height,
            dest_rect.left,
            dest_rect.top,
        );
        let transform = prepare_transform(transform)?.post_concatenated(&placement);
        transform
            .validate()
            .map_err(|_| ShadingRasterError::InvalidInput("nonfinite transform"))?;
        Ok(Self::RasterImage { image, transform })
    }

    /// Returns a shallow clone with `parent` applied after the paint transform.
    pub fn with_parent_transform(&self, parent: &Transform) -> Self {
        let mut paint = self.clone();
        match &mut paint {
            Self::LinearGradient { transform, .. }
            | Self::RadialGradient { transform, .. }
            | Self::RasterImage { transform, .. } => {
                *transform = parent.post_concatenated(transform);
            }
        }
        paint
    }

    /// Checks gradient geometry and stops, or raster dimensions, data length, and bounds.
    ///
    /// # Errors
    ///
    /// Returns [`ShadingRasterError::InvalidInput`] for degenerate or nonfinite gradient
    /// geometry, invalid stops, empty raster dimensions, mismatched RGBA8 data length,
    /// or invalid raster bounds. Returns [`ShadingRasterError::ResourceLimit`] if the
    /// expected raster buffer length overflows.
    pub fn validate(&self) -> Result<(), ShadingRasterError> {
        let (positions, colors): (&[f32], &[Color]) = match self {
            Self::LinearGradient {
                x0,
                y0,
                x1,
                y1,
                positions,
                colors,
                ..
            } => {
                if x0 == x1 && y0 == y1 {
                    return Err(ShadingRasterError::InvalidInput(
                        "degenerate linear gradient",
                    ));
                }
                if [*x0, *y0, *x1, *y1].iter().any(|v| !v.is_finite()) {
                    return Err(ShadingRasterError::InvalidInput("gradient coordinates"));
                }
                (positions, colors)
            }
            Self::RadialGradient {
                start_x,
                start_y,
                start_r,
                end_x,
                end_y,
                end_r,
                positions,
                colors,
                ..
            } => {
                if *start_r < 0.0
                    || *end_r < 0.0
                    || (start_x == end_x && start_y == end_y && start_r == end_r)
                {
                    return Err(ShadingRasterError::InvalidInput(
                        "degenerate radial gradient",
                    ));
                }
                if [*start_x, *start_y, *start_r, *end_x, *end_y, *end_r]
                    .iter()
                    .any(|v| !v.is_finite())
                {
                    return Err(ShadingRasterError::InvalidInput("gradient coordinates"));
                }
                (positions, colors)
            }
            Self::RasterImage { image, transform } => {
                if image.width == 0 || image.height == 0 {
                    return Err(ShadingRasterError::InvalidInput("empty image"));
                }
                if image.pixel_format != PixelFormat::RGBA8888
                    || image.data.len()
                        != image
                            .rgba_byte_size()
                            .ok_or(ShadingRasterError::ResourceLimit)?
                {
                    return Err(ShadingRasterError::InvalidInput("image byte length"));
                }
                transform
                    .validate()
                    .map_err(|_| ShadingRasterError::InvalidInput("nonfinite transform"))?;
                return Ok(());
            }
        };
        validate_stops(positions, colors)?;
        let transform = match self {
            Self::LinearGradient { transform, .. } | Self::RadialGradient { transform, .. } => {
                transform
            }
            Self::RasterImage { .. } => return Ok(()),
        };
        transform
            .validate()
            .map_err(|_| ShadingRasterError::InvalidInput("nonfinite transform"))
    }
}

fn prepare_transform(transform: Option<Transform>) -> Result<Transform, ShadingRasterError> {
    let transform = transform.unwrap_or_else(Transform::identity);
    transform
        .try_inverse()
        .map_err(|_| ShadingRasterError::InvalidInput("invalid transform"))?;
    Ok(transform)
}

fn normalized_colors(colors: &[Color]) -> Result<Arc<[Color]>, ShadingRasterError> {
    colors
        .iter()
        .map(|color| {
            if [color.r, color.g, color.b, color.a]
                .iter()
                .any(|value| !value.is_finite())
            {
                return Err(ShadingRasterError::InvalidInput("gradient color"));
            }
            Ok(Color::from_rgba(
                color.r.clamp(0.0, 1.0),
                color.g.clamp(0.0, 1.0),
                color.b.clamp(0.0, 1.0),
                color.a.clamp(0.0, 1.0),
            ))
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Arc::from)
}

/// Builds backend-facing paint data for a parsed shading and optional transform.
pub fn build_shading_paint(
    shading: &Shading,
    transform: Option<Transform>,
) -> Result<ShadingPaint, PdfShadingError> {
    match shading {
        Shading::Axial {
            coords,
            color_stops,
            ..
        } => ShadingPaint::linear_gradient(
            *coords,
            transform,
            Arc::clone(&color_stops.positions),
            Arc::clone(&color_stops.colors),
        )
        .map_err(|error| PdfShadingError::UnsupportedFeature(error.to_string())),
        Shading::Radial {
            coords,
            color_stops,
            ..
        } => ShadingPaint::radial_gradient(
            *coords,
            transform,
            Arc::clone(&color_stops.positions),
            Arc::clone(&color_stops.colors),
        )
        .map_err(|error| PdfShadingError::UnsupportedFeature(error.to_string())),
        Shading::FunctionBased { .. } => Err(PdfShadingError::UnsupportedFeature(
            "FunctionBased shading not implemented".to_string(),
        )),
        Shading::FreeFormTriangleMesh {
            bbox, triangles, ..
        } => {
            let mesh_transform = match transform {
                Some(value) => value,
                None => Transform::identity(),
            };
            let bounds = bbox
                .map(|rect| mesh_transform.map_rect(&rect))
                .filter(has_paintable_bounds)
                .or_else(|| triangle_mesh_bounds(triangles, &mesh_transform))
                .filter(has_paintable_bounds);
            let Some(bounds) = bounds.map(|value| value.normalized()) else {
                return Ok(transparent_raster_paint());
            };

            let raster = rasterize_mesh_triangles(triangles, bounds, &mesh_transform);

            ShadingPaint::raster_image(
                Image {
                    data: raster.pixels.into(),
                    width: raster.width,
                    height: raster.height,
                    pixel_format: PixelFormat::RGBA8888,
                },
                raster.bounds,
                None,
            )
            .map_err(|error| PdfShadingError::UnsupportedFeature(error.to_string()))
        }
        Shading::PatchMesh { bbox, patches, .. } => {
            let mesh_transform = match transform {
                Some(value) => value,
                None => Transform::identity(),
            };
            let mut bounds = bbox.map(|rect| mesh_transform.map_rect(&rect));
            if bounds.is_none() {
                bounds = patch_mesh_bounds(
                    patches.iter().map(MeshPatchRef::from_patch),
                    &mesh_transform,
                );
            }
            let bounds = bounds.ok_or_else(|| {
                PdfShadingError::UnsupportedFeature("Patch mesh has no bounds".to_string())
            })?;
            let bounds = bounds.normalized();
            if bounds.width() <= 0.0 || bounds.height() <= 0.0 {
                return Err(PdfShadingError::UnsupportedFeature(
                    "Patch mesh bounds are empty".to_string(),
                ));
            }

            let raster = rasterize_mesh_patches(
                patches.iter().map(MeshPatchRef::from_patch),
                bounds,
                &mesh_transform,
            );

            ShadingPaint::raster_image(
                Image {
                    data: raster.pixels.into(),
                    width: raster.width,
                    height: raster.height,
                    pixel_format: PixelFormat::RGBA8888,
                },
                raster.bounds,
                None,
            )
            .map_err(|error| PdfShadingError::UnsupportedFeature(error.to_string()))
        }
        Shading::Unsupported { name } => Err(PdfShadingError::UnsupportedFeature(format!(
            "Shading type '{name}' not implemented"
        ))),
    }
}

fn has_paintable_bounds(bounds: &Rect) -> bool {
    let normalized = bounds.normalized();
    normalized.left.is_finite()
        && normalized.top.is_finite()
        && normalized.right.is_finite()
        && normalized.bottom.is_finite()
        && normalized.width() > 0.0
        && normalized.height() > 0.0
}

fn transparent_raster_paint() -> ShadingPaint {
    ShadingPaint::RasterImage {
        image: Image {
            data: Bytes::from_static(&[0_u8, 0, 0, 0]),
            width: 1,
            height: 1,
            pixel_format: PixelFormat::RGBA8888,
        },
        transform: Transform::identity(),
    }
}
