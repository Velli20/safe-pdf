//! Backend-facing shading paint descriptions and builders.

use bytes::Bytes;
use pdf_graphics::{Image, PixelFormat, point::Point, transform::Transform};
use pdf_image::ImageRasterError;

use crate::{color_stops::ColorStops, error::PdfShadingError, mesh::paint_mesh, model::Shading};

/// A backend-ready representation of a parsed PDF shading.
///
/// Equality compares field values, including the contents of shared buffers.
/// Floating-point fields use ordinary equality, so paints containing NaN may
/// compare unequal to themselves and this type does not implement `Eq`.
#[derive(Clone, PartialEq)]
pub struct ShadingPaint {
    /// What is painted, in shading or source-pixel coordinates.
    pub source: ShadingSource,
    /// Transform mapping the source's coordinates into device space.
    pub transform: Transform,
}

/// The colors of a [`ShadingPaint`] before its transform is applied.
#[derive(Clone, PartialEq)]
pub enum ShadingSource {
    /// Colors running along the line from `start` to `end`.
    LinearGradient {
        /// Where the first stop is painted.
        start: Point,
        /// Where the last stop is painted.
        end: Point,
        /// The colors between both points.
        stops: ColorStops,
    },
    /// Colors blending from one circle into another.
    RadialGradient {
        /// Center of the circle painted with the first stop.
        start: Point,
        /// Radius of the circle painted with the first stop.
        start_radius: f32,
        /// Center of the circle painted with the last stop.
        end: Point,
        /// Radius of the circle painted with the last stop.
        end_radius: f32,
        /// The colors between both circles.
        stops: ColorStops,
    },
    /// A rasterized shading as render-ready RGBA8 pixels.
    RasterImage(Image),
}

impl ShadingPaint {
    /// Builds a validated linear-gradient paint with clamped colors from
    /// `[x0, y0, x1, y1]`.
    pub fn linear_gradient(
        [x0, y0, x1, y1]: [f32; 4],
        stops: &ColorStops,
        transform: Transform,
    ) -> Result<Self, ImageRasterError> {
        let source = ShadingSource::LinearGradient {
            start: Point::new(x0, y0),
            end: Point::new(x1, y1),
            stops: stops.clamped(),
        };
        source.into_paint(transform)
    }

    /// Builds a validated radial-gradient paint with clamped colors from
    /// `[x0, y0, r0, x1, y1, r1]`.
    pub fn radial_gradient(
        [x0, y0, start_radius, x1, y1, end_radius]: [f32; 6],
        stops: &ColorStops,
        transform: Transform,
    ) -> Result<Self, ImageRasterError> {
        let source = ShadingSource::RadialGradient {
            start: Point::new(x0, y0),
            start_radius,
            end: Point::new(x1, y1),
            end_radius,
            stops: stops.clamped(),
        };
        source.into_paint(transform)
    }

    /// Builds a validated raster paint whose transform maps image pixels into device space.
    pub fn raster_image(image: Image, transform: Transform) -> Result<Self, ImageRasterError> {
        ShadingSource::RasterImage(image).into_paint(transform)
    }

    /// Returns a paint that leaves everything it covers unchanged.
    pub(crate) fn transparent() -> Self {
        Self {
            source: ShadingSource::RasterImage(Image {
                data: Bytes::from_static(&[0; 4]),
                width: 1,
                height: 1,
                pixel_format: PixelFormat::RGBA8888,
                interpolate: false,
            }),
            transform: Transform::identity(),
        }
    }

    /// Returns a shallow clone with `parent` applied after the paint transform.
    pub fn with_parent_transform(&self, parent: &Transform) -> Self {
        Self {
            source: self.source.clone(),
            transform: parent.post_concatenated(&self.transform),
        }
    }

    /// Checks gradient geometry and stops, or raster dimensions, format, and data length,
    /// and that the transform can be inverted.
    ///
    /// # Errors
    ///
    /// Returns [`ImageRasterError::InvalidInput`] for degenerate or nonfinite gradient
    /// geometry, invalid stops, or a non-RGBA8 raster; [`ImageRasterError::Image`] for
    /// a malformed raster; and [`ImageRasterError::Transform`] for a nonfinite or
    /// singular transform.
    pub fn validate(&self) -> Result<(), ImageRasterError> {
        self.source.validate()?;
        // Sampling maps device points back into shading or source-pixel space.
        self.transform.try_inverse()?;
        Ok(())
    }
}

impl ShadingSource {
    /// Pairs this source with its transform and validates the paint.
    fn into_paint(self, transform: Transform) -> Result<ShadingPaint, ImageRasterError> {
        let paint = ShadingPaint {
            source: self,
            transform,
        };
        paint.validate()?;
        Ok(paint)
    }

    fn validate(&self) -> Result<(), ImageRasterError> {
        match self {
            Self::LinearGradient { start, end, stops } => {
                if start == end {
                    return Err(ImageRasterError::InvalidInput("degenerate linear gradient"));
                }
                if !start.is_finite() || !end.is_finite() {
                    return Err(ImageRasterError::InvalidInput("gradient coordinates"));
                }
                stops.validate()
            }
            Self::RadialGradient {
                start,
                start_radius,
                end,
                end_radius,
                stops,
            } => {
                if *start_radius < 0.0
                    || *end_radius < 0.0
                    || (start, start_radius) == (end, end_radius)
                {
                    return Err(ImageRasterError::InvalidInput("degenerate radial gradient"));
                }
                if !start.is_finite()
                    || !end.is_finite()
                    || !start_radius.is_finite()
                    || !end_radius.is_finite()
                {
                    return Err(ImageRasterError::InvalidInput("gradient coordinates"));
                }
                stops.validate()
            }
            Self::RasterImage(image) => {
                image.validate()?;
                if image.pixel_format != PixelFormat::RGBA8888 {
                    return Err(ImageRasterError::InvalidInput("raster image format"));
                }
                Ok(())
            }
        }
    }
}

impl Shading {
    /// Builds the backend-facing paint of this shading, mapped into device space by
    /// `transform`.
    ///
    /// Gradients paint `/Background` past each end that does not extend when
    /// `use_background` is set, and nothing there otherwise. A shading painted with the
    /// `sh` operator ignores its background, so only pattern fills set it.
    pub fn paint(
        &self,
        transform: &Transform,
        use_background: bool,
    ) -> Result<ShadingPaint, PdfShadingError> {
        let paint = match self {
            Self::Axial { coords, gradient } => ShadingPaint::linear_gradient(
                *coords,
                &gradient.bounded_stops(use_background),
                *transform,
            )?,
            Self::Radial { coords, gradient } => ShadingPaint::radial_gradient(
                *coords,
                &gradient.bounded_stops(use_background),
                *transform,
            )?,
            Self::FunctionBased(shading) => shading.paint(transform, use_background)?,
            Self::FreeFormTriangleMesh {
                bbox, triangles, ..
            } => paint_mesh(triangles, *bbox, transform)?,
            Self::PatchMesh { bbox, patches, .. } => paint_mesh(patches, *bbox, transform)?,
            Self::Unsupported(shading_type) => {
                return Err(PdfShadingError::UnsupportedShadingType(*shading_type));
            }
        };
        Ok(paint)
    }
}
