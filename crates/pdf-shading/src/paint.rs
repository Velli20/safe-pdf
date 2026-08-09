//! Backend-facing shading paint descriptions and builders.

use std::sync::Arc;

use pdf_graphics::{color::Color, rect::Rect, transform::Transform};

use crate::{
    error::PdfShadingError,
    mesh::{
        MeshPatchRef, patch_mesh_bounds, rasterize_mesh_patches, rasterize_mesh_triangles,
        triangle_mesh_bounds,
    },
    model::Shading,
};

/// A backend-ready representation of a parsed PDF shading.
#[derive(Clone)]
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
        /// Optional transform mapping shading space into device space.
        transform: Option<Transform>,
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
        /// Optional transform mapping shading space into device space.
        transform: Option<Transform>,
    },
    /// A rasterized mesh shading paint.
    RasterImage {
        /// Shared RGBA8 image data for the rasterized shading.
        pixels: Arc<Vec<u8>>,
        /// Raster width in pixels.
        width: usize,
        /// Raster height in pixels.
        height: usize,
        /// Destination rectangle in device space.
        dest_rect: Rect,
        /// Optional local transform.
        transform: Option<Transform>,
    },
}

/// Builds backend-facing paint data for a parsed shading and optional transform.
pub fn build_shading_paint(
    shading: &Shading,
    transform: Option<Transform>,
) -> Result<ShadingPaint, PdfShadingError> {
    match shading {
        Shading::Axial {
            coords: [x0, y0, x1, y1],
            color_stops,
            ..
        } => Ok(ShadingPaint::LinearGradient {
            x0: *x0,
            y0: *y0,
            x1: *x1,
            y1: *y1,
            colors: Arc::clone(&color_stops.colors),
            positions: Arc::clone(&color_stops.positions),
            transform,
        }),
        Shading::Radial {
            coords: [start_x, start_y, start_r, end_x, end_y, end_r],
            color_stops,
            ..
        } => Ok(ShadingPaint::RadialGradient {
            start_x: *start_x,
            start_y: *start_y,
            start_r: *start_r,
            end_x: *end_x,
            end_y: *end_y,
            end_r: *end_r,
            colors: Arc::clone(&color_stops.colors),
            positions: Arc::clone(&color_stops.positions),
            transform,
        }),
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

            Ok(ShadingPaint::RasterImage {
                pixels: Arc::new(raster.pixels),
                width: raster.width,
                height: raster.height,
                dest_rect: raster.bounds,
                transform: None,
            })
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

            Ok(ShadingPaint::RasterImage {
                pixels: Arc::new(raster.pixels),
                width: raster.width,
                height: raster.height,
                dest_rect: raster.bounds,
                transform: None,
            })
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
        pixels: Arc::new(vec![0_u8, 0, 0, 0]),
        width: 1,
        height: 1,
        dest_rect: Rect {
            left: 0.0,
            top: 0.0,
            right: 1.0,
            bottom: 1.0,
        },
        transform: None,
    }
}
