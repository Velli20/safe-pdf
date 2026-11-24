//! Software sampling of backend-facing shading paints.
//!
//! Linear and radial gradients are evaluated from their geometry and color stops;
//! raster paints are sampled directly from their existing RGBA8 data. Each output
//! pixel uses one sample at its center, without antialiasing or supersampling.
//! Sampling and allocation are independent of rendering backends. Callers choose
//! the output resolution and enforce their own memory budgets.

use pdf_graphics::{Image, PixelFormat, point::Point, rect::Rect};
use pdf_image::{ImageRasterError as Error, raster};

use crate::paint::{ShadingPaint, ShadingSource};

/// Samples a paint at pixel centers over `bounds`, returning straight-alpha RGBA8 pixels.
///
/// The paint transform maps shading or source-image coordinates into the coordinate
/// space of `bounds`. Samples are mapped back through its inverse before evaluating
/// the paint. `size` specifies `[width, height]`,
/// independently of the bounds' dimensions. Pixels are packed in row-major order,
/// with the first row sampling nearest `bounds.top`.
///
/// Gradient parameters past the stops take the nearest end stop's color.
/// Radial samples without a valid circle intersection are transparent. Raster
/// paints use nearest-neighbor sampling in source-pixel coordinates. Samples outside
/// the source image's half-open bounds are transparent.
///
/// # Errors
///
/// Returns [`Error::InvalidInput`] for invalid paint data, nonpositive or nonfinite
/// output bounds or zero output dimensions, and [`Error::Transform`] for singular or
/// nonfinite transforms, or if the bounds cannot be mapped back with finite coordinates.
/// Returns [`Error::ResourceLimit`] if the buffer size overflows and
/// [`Error::Allocation`] if allocation fails. No fixed resolution or memory cap is
/// imposed by this function.
pub fn rasterize_shading(
    paint: &ShadingPaint,
    bounds: &Rect,
    size: [u32; 2],
) -> Result<Image, Error> {
    paint.validate()?;
    if !bounds.is_valid() {
        return Err(Error::InvalidInput("sample bounds"));
    }
    let [w, h] = size;
    if w == 0 || h == 0 {
        return Err(Error::InvalidInput("empty image"));
    }
    let (width, height) = (usize::try_from(w)?, usize::try_from(h)?);
    // Reserve the output before the per-axis centers so oversized requests fail first.
    let mut data = raster::pixel_buffer(width, height, 4)?;
    let columns = raster::pixel_centers(bounds.left, bounds.width(), w)?;
    let rows = raster::pixel_centers(bounds.top, bounds.height(), h)?;
    // Invert once per image: paint transforms map out of shading space, while
    // sampling needs the opposite direction.
    let local = paint.transform.try_inverse()?;
    // Finite corners keep every sample between them finite.
    local.try_map_rect(bounds)?;
    for &y in &rows {
        for &x in &columns {
            data.extend_from_slice(&paint.source.sample(local.map_point(Point::new(x, y))));
        }
    }
    Ok(Image {
        width,
        height,
        pixel_format: PixelFormat::RGBA8888,
        interpolate: false,
        data: data.into(),
    })
}

impl ShadingSource {
    /// Evaluates a validated source at a finite point in its own coordinates.
    ///
    /// Returns straight-alpha RGBA8, using transparent black for uncovered samples.
    fn sample(&self, point: Point) -> [u8; 4] {
        let sample = match self {
            Self::LinearGradient { start, end, stops } => {
                let t = linear_parameter(*start, *end, point);
                Some(stops.color_at(t).to_rgba8())
            }
            Self::RadialGradient {
                start,
                start_radius,
                end,
                end_radius,
                stops,
            } => radial_parameter((*start, *start_radius), (*end, *end_radius), point)
                .map(|t| stops.color_at(t).to_rgba8()),
            Self::RasterImage(image) => raster::nearest_pixel(image, point.x, point.y),
        };
        sample.unwrap_or_default()
    }
}

/// Returns the parameter of `point` projected onto the axis from `start` to `end`.
fn linear_parameter(start: Point, end: Point, point: Point) -> f64 {
    // t = dot(point - start, end - start) / |end - start|².
    let [x, y] = offset(start, point);
    let [dx, dy] = offset(start, end);
    (x * dx + y * dy) / (dx * dx + dy * dy)
}

/// Returns the radial gradient parameter of the circle through `point`, if one exists.
///
/// `start` and `end` hold each circle's center and radius.
fn radial_parameter(
    (start, start_radius): (Point, f32),
    (end, end_radius): (Point, f32),
    point: Point,
) -> Option<f64> {
    let [x, y] = offset(start, point);
    let [dx, dy] = offset(start, end);
    let r = f64::from(start_radius);
    let dr = f64::from(end_radius) - r;
    // A circle at t has center start + t * (dx, dy) and radius r + t * dr.
    // Expanding |(x, y) - t * (dx, dy)|² = (r + t * dr)² gives a*t² + b*t + c = 0.
    let a = dx * dx + dy * dy - dr * dr;
    let b = -2.0 * (x * dx + y * dy + r * dr);
    let c = x * x + y * y - r * r;
    // Infinite and NaN roots mark the equations without a solution. A nearly zero
    // quadratic coefficient leaves a linear equation.
    let roots = if a.abs() >= 1e-12 {
        let root = (b * b - 4.0 * a * c).sqrt();
        [(-b - root) / (2.0 * a), (-b + root) / (2.0 * a)]
    } else if b.abs() >= 1e-12 {
        [-c / b; 2]
    } else {
        return None;
    };
    // Discard non-real solutions and circles with negative radius. When
    // both roots survive, prefer the larger-radius circle: the smaller t
    // for shrinking radii, otherwise the larger t (also used for equal radii).
    roots
        .into_iter()
        .filter(|t| t.is_finite() && r + t * dr >= 0.0)
        .reduce(|a, b| if dr < 0.0 { a.min(b) } else { a.max(b) })
}

/// Returns the displacement between two points in the precision parameters are solved in.
fn offset(from: Point, to: Point) -> [f64; 2] {
    [
        f64::from(to.x) - f64::from(from.x),
        f64::from(to.y) - f64::from(from.y),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color_stops::ColorStops;
    use pdf_graphics::{color::Color, transform::Transform};

    fn stops() -> ColorStops {
        ColorStops {
            positions: [0.0, 1.0].into(),
            colors: [
                Color::from_rgb(0.0, 0.0, 0.0),
                Color::from_rgb(1.0, 1.0, 1.0),
            ]
            .into(),
        }
    }

    fn linear(transform: Transform) -> ShadingPaint {
        ShadingPaint::linear_gradient([0.0, 0.0, 1.0, 0.0], &stops(), transform).unwrap()
    }

    fn radial(start_r: f32, end_x: f32, end_r: f32) -> ShadingPaint {
        ShadingPaint {
            source: ShadingSource::RadialGradient {
                start: Point::new(0.0, 0.0),
                start_radius: start_r,
                end: Point::new(end_x, 0.0),
                end_radius: end_r,
                stops: stops(),
            },
            transform: Transform::identity(),
        }
    }

    #[test]
    fn linear_samples_centers_at_requested_resolution() {
        let image =
            rasterize_shading(&linear(Transform::identity()), &Rect::UNIT_RECT, [2, 1]).unwrap();
        assert_eq!((image.width, image.height), (2, 1));
        assert_eq!(image.pixel_format, PixelFormat::RGBA8888);
        assert_eq!(image.data.as_ref(), &[64, 64, 64, 255, 191, 191, 191, 255]);
        let image = rasterize_shading(
            &linear(Transform::identity()),
            &Rect::from([-1.0, 0.0, 2.0, 1.0]),
            [3, 1],
        )
        .unwrap();
        assert_eq!(
            image.data.as_ref(),
            &[0, 0, 0, 255, 128, 128, 128, 255, 255, 255, 255, 255]
        );
    }

    #[test]
    fn transformed_gradient_matches_local_samples() {
        // A quarter turn, nonuniform scale, and translation turn the horizontal ramp vertical.
        let transform = Transform::from_row(0.0, 2.0, -3.0, 0.0, 8.0, 10.0);
        let image = rasterize_shading(
            &linear(transform),
            &Rect::from([5.0, 10.0, 8.0, 12.0]),
            [1, 2],
        )
        .unwrap();
        assert_eq!(image.data.as_ref(), &[64, 64, 64, 255, 191, 191, 191, 255]);
    }

    #[test]
    fn radial_handles_expanding_shrinking_and_linear_equations() {
        assert_eq!(
            radial(0.0, 0.0, 2.0).source.sample(Point::new(1.0, 0.0)),
            [128, 128, 128, 255]
        );
        assert_eq!(
            radial(2.0, 0.0, 0.0).source.sample(Point::new(0.5, 0.0)),
            [191, 191, 191, 255]
        );
        assert_eq!(
            radial(0.0, 1.0, 1.0).source.sample(Point::new(1.0, 0.0)),
            [128, 128, 128, 255]
        );
        assert_eq!(
            radial(1.0, 2.0, 1.0).source.sample(Point::new(0.0, 2.0)),
            [0; 4]
        );
        let image = rasterize_shading(
            &radial(0.0, 0.0, 2.0),
            &Rect::from([0.5, -0.5, 1.5, 0.5]),
            [1, 1],
        )
        .unwrap();
        assert_eq!(image.data.as_ref(), &[128, 128, 128, 255]);
    }

    #[test]
    fn raster_sampling_preserves_pixels_and_transparent_exterior() {
        let paint = ShadingPaint::raster_image(
            Image {
                data: vec![255, 0, 0, 64, 0, 0, 255, 255].into(),
                width: 2,
                height: 1,
                pixel_format: PixelFormat::RGBA8888,
                interpolate: false,
            },
            Transform::from_translate(3.0, 4.0),
        )
        .unwrap();
        let image = rasterize_shading(&paint, &Rect::from([2.0, 4.0, 6.0, 5.0]), [4, 1]).unwrap();
        assert_eq!(
            image.data.as_ref(),
            &[0, 0, 0, 0, 255, 0, 0, 64, 0, 0, 255, 255, 0, 0, 0, 0]
        );
    }

    #[test]
    fn rejects_invalid_geometry_and_dimensions() {
        for bounds in [
            Rect::new(0.0, 1.0),
            Rect::new(-1.0, 1.0),
            Rect::new(f32::NAN, 1.0),
            Rect::new(f32::INFINITY, 1.0),
        ] {
            assert!(matches!(
                rasterize_shading(&linear(Transform::identity()), &bounds, [1, 1]),
                Err(Error::InvalidInput(_))
            ));
        }
        assert!(matches!(
            rasterize_shading(&linear(Transform::identity()), &Rect::UNIT_RECT, [0, 1]),
            Err(Error::InvalidInput(_))
        ));
        assert!(matches!(
            rasterize_shading(
                &linear(Transform::identity()),
                &Rect::UNIT_RECT,
                [u32::MAX; 2]
            ),
            Err(Error::ResourceLimit)
        ));
        for transform in [
            Transform::from_scale(0.0, 1.0),
            Transform::from_translate(f32::NAN, 0.0),
            Transform::from_scale(f32::INFINITY, 1.0),
        ] {
            assert!(matches!(
                rasterize_shading(
                    &ShadingPaint {
                        transform,
                        ..linear(Transform::identity())
                    },
                    &Rect::UNIT_RECT,
                    [1, 1]
                ),
                Err(Error::Transform(_))
            ));
        }
        for x1 in [0.0, f32::NAN] {
            assert!(
                ShadingPaint::linear_gradient([0.0, 0.0, x1, 0.0], &stops(), Transform::identity())
                    .is_err()
            );
        }
        for paint in [
            radial(1.0, 0.0, 1.0),
            radial(-1.0, 0.0, 1.0),
            radial(0.0, f32::NAN, 1.0),
        ] {
            assert!(paint.validate().is_err());
        }
    }

    #[test]
    fn rejects_malformed_raster_data() {
        for (width, height) in [(1, 1), (0, 1), (usize::MAX, 2)] {
            let paint = ShadingPaint {
                source: ShadingSource::RasterImage(Image {
                    data: vec![0; 3].into(),
                    width,
                    height,
                    pixel_format: PixelFormat::RGBA8888,
                    interpolate: false,
                }),
                transform: Transform::identity(),
            };
            assert!(rasterize_shading(&paint, &Rect::UNIT_RECT, [1, 1]).is_err());
        }
        let paint = ShadingPaint {
            source: ShadingSource::RasterImage(Image {
                data: vec![0; 4].into(),
                width: 1,
                height: 1,
                pixel_format: PixelFormat::Gray8,
                interpolate: false,
            }),
            transform: Transform::identity(),
        };
        assert!(paint.validate().is_err());
    }
}
