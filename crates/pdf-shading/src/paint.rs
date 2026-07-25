//! Backend-facing shading paint descriptions and builders.

use std::{borrow::Cow, sync::Arc};

use pdf_graphics::{color::Color, rect::Rect, transform::Transform};

use crate::{
    error::PdfShadingError,
    mesh::{MeshPatchRef, patch_mesh_bounds, rasterize_mesh_patches},
    model::Shading,
};

/// A backend-ready representation of a parsed PDF shading.
#[derive(Clone)]
pub enum ShadingPaint<'a> {
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
        colors: Cow<'a, [Color]>,
        /// Gradient stop positions.
        positions: Cow<'a, [f32]>,
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
        colors: Cow<'a, [Color]>,
        /// Gradient stop positions.
        positions: Cow<'a, [f32]>,
        /// Optional transform mapping shading space into device space.
        transform: Option<Transform>,
    },
    /// A rasterized mesh shading paint.
    RasterImage {
        /// Shared RGBA8 image data for the rasterized shading.
        pixels: Arc<[u8]>,
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

impl<'a> ShadingPaint<'a> {
    /// Converts this shading paint into an owned `'static` value.
    pub fn to_static(&self) -> ShadingPaint<'static> {
        match self {
            Self::LinearGradient {
                x0,
                y0,
                x1,
                y1,
                transform,
                colors,
                positions,
            } => ShadingPaint::LinearGradient {
                x0: *x0,
                y0: *y0,
                x1: *x1,
                y1: *y1,
                transform: *transform,
                colors: Cow::Owned(colors.to_vec()),
                positions: Cow::Owned(positions.to_vec()),
            },
            Self::RadialGradient {
                start_x,
                start_y,
                start_r,
                end_x,
                end_y,
                end_r,
                colors,
                positions,
                transform,
            } => ShadingPaint::RadialGradient {
                start_x: *start_x,
                start_y: *start_y,
                start_r: *start_r,
                end_x: *end_x,
                end_y: *end_y,
                end_r: *end_r,
                colors: Cow::Owned(colors.to_vec()),
                positions: Cow::Owned(positions.to_vec()),
                transform: *transform,
            },
            Self::RasterImage {
                pixels,
                width,
                height,
                dest_rect,
                transform,
            } => ShadingPaint::RasterImage {
                pixels: Arc::clone(pixels),
                width: *width,
                height: *height,
                dest_rect: *dest_rect,
                transform: *transform,
            },
        }
    }
}

/// Builds backend-facing paint data for a parsed shading and optional transform.
pub fn build_shading_paint<'a>(
    shading: &'a Shading,
    transform: Option<Transform>,
) -> Result<ShadingPaint<'a>, PdfShadingError> {
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
            colors: Cow::Borrowed(&color_stops.colors),
            positions: Cow::Borrowed(&color_stops.positions),
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
            colors: Cow::Borrowed(&color_stops.colors),
            positions: Cow::Borrowed(&color_stops.positions),
            transform,
        }),
        Shading::FunctionBased { .. } => Err(PdfShadingError::UnsupportedFeature(
            "FunctionBased shading not implemented".to_string(),
        )),
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
                pixels: raster.pixels.into(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raster_image_to_static_shares_pixels() {
        let pixels: Arc<[u8]> = vec![1, 2, 3, 4].into();
        let paint = ShadingPaint::RasterImage {
            pixels: Arc::clone(&pixels),
            width: 1,
            height: 1,
            dest_rect: Rect::UNIT_RECT,
            transform: None,
        };

        let static_paint = paint.to_static();
        assert!(matches!(&static_paint, ShadingPaint::RasterImage { .. }));
        if let ShadingPaint::RasterImage {
            pixels: static_pixels,
            ..
        } = static_paint
        {
            assert!(Arc::ptr_eq(&static_pixels, &pixels));
        }
    }
}
