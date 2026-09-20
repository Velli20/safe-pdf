//! Software sampling of backend-facing shading paints.
//!
//! Linear and radial gradients are evaluated from their geometry and color stops;
//! raster paints are sampled directly from their existing RGBA8 data. Each output
//! pixel uses one sample at its center, without antialiasing or supersampling.
//! Sampling and allocation are independent of rendering backends. Callers choose
//! the output resolution and enforce their own memory budgets.

use crate::{color_stops::interpolate, error::ShadingRasterError as Error, paint::ShadingPaint};
use num_traits::ToPrimitive;
use pdf_graphics::{Image, PixelFormat, point::Point, rect::Rect, transform::Transform};

/// Samples a paint at pixel centers over `bounds`, returning straight-alpha RGBA8 pixels.
///
/// The paint transform maps shading or source-image coordinates into the coordinate
/// space of `bounds`. Samples are mapped back through its inverse before evaluating
/// the paint. `size` specifies `[width, height]`,
/// independently of the bounds' dimensions. Pixels are packed in row-major order,
/// with the first row sampling nearest `bounds.top`.
///
/// Gradient parameters are clamped to `0.0..=1.0` before interpolating stops.
/// Radial samples without a valid circle intersection are transparent. Raster
/// paints use nearest-neighbor sampling in source-pixel coordinates. Samples outside
/// the source image's half-open bounds are transparent.
///
/// # Errors
///
/// Returns [`Error::InvalidInput`] for invalid paint data, nonpositive or nonfinite
/// output bounds, zero output dimensions, or transforms and mapped points that
/// cannot be represented with finite coordinates. Singular transforms are rejected.
/// Returns [`Error::ResourceLimit`] if dimensions or buffer offsets overflow, or
/// allocation fails. No fixed resolution or memory cap is imposed by this function.
pub fn rasterize_shading(
    paint: &ShadingPaint,
    bounds: &Rect,
    size: [u32; 2],
) -> Result<Image, Error> {
    paint.validate()?;
    if !bounds.is_valid() || !bounds.width().is_finite() || !bounds.height().is_finite() {
        return Err(Error::InvalidInput("sample bounds"));
    }
    let [w, h] = size;
    if w == 0 || h == 0 {
        return Err(Error::InvalidInput("empty image"));
    }
    let bytes = usize::try_from(w)
        .map_err(|_| Error::ResourceLimit)?
        .checked_mul(usize::try_from(h).map_err(|_| Error::ResourceLimit)?)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or(Error::ResourceLimit)?;
    // Invert once per image: paint transforms map out of shading space, while
    // sampling needs the opposite direction. Reject invalid transforms before allocation.
    let local = match paint {
        ShadingPaint::LinearGradient { transform, .. }
        | ShadingPaint::RadialGradient { transform, .. }
        | ShadingPaint::RasterImage { transform, .. } => transform
            .try_inverse()
            .map_err(|_| Error::InvalidInput("singular transform"))?,
    };
    let mut data = Vec::new();
    data.try_reserve_exact(bytes)
        .map_err(|_| Error::ResourceLimit)?;
    for y in 0..h {
        for x in 0..w {
            // Map pixel centers across the full bounds at the requested resolution.
            // Use f64 for the intermediate arithmetic before mapping through the f32 transform.
            let px = f64::from(bounds.left)
                + (f64::from(x) + 0.5) * f64::from(bounds.width()) / f64::from(w);
            let py = f64::from(bounds.top)
                + (f64::from(y) + 0.5) * f64::from(bounds.height()) / f64::from(h);
            let p = map_point(
                &local,
                Point::new(
                    px.to_f32().ok_or(Error::InvalidInput("sample x"))?,
                    py.to_f32().ok_or(Error::InvalidInput("sample y"))?,
                ),
            )?;
            data.extend_from_slice(&sample(paint, p)?);
        }
    }
    Ok(Image {
        width: usize::try_from(w).map_err(|_| Error::ResourceLimit)?,
        height: usize::try_from(h).map_err(|_| Error::ResourceLimit)?,
        pixel_format: PixelFormat::RGBA8888,
        data: data.into(),
    })
}

/// Evaluates a validated paint at a finite point already mapped into shading space.
///
/// Returns straight-alpha RGBA8, using transparent black for uncovered samples.
/// The caller must validate the paint and apply its inverse transform first.
fn sample(paint: &ShadingPaint, p: Point) -> Result<[u8; 4], Error> {
    let (t, positions, colors) = match paint {
        ShadingPaint::LinearGradient {
            x0,
            y0,
            x1,
            y1,
            positions,
            colors,
            ..
        } => {
            // Project onto the gradient axis: t = dot(p - start, end - start) / |end - start|².
            let dx = f64::from(*x1) - f64::from(*x0);
            let dy = f64::from(*y1) - f64::from(*y0);
            let t = ((f64::from(p.x) - f64::from(*x0)) * dx
                + (f64::from(p.y) - f64::from(*y0)) * dy)
                / (dx * dx + dy * dy);
            (t, positions, colors)
        }
        ShadingPaint::RadialGradient {
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
            let x = f64::from(p.x) - f64::from(*start_x);
            let y = f64::from(p.y) - f64::from(*start_y);
            let dx = f64::from(*end_x) - f64::from(*start_x);
            let dy = f64::from(*end_y) - f64::from(*start_y);
            let r = f64::from(*start_r);
            let dr = f64::from(*end_r) - r;
            // A circle at t has center start + t * (dx, dy) and radius r + t * dr.
            // Expanding |(x, y) - t * (dx, dy)|² = (r + t * dr)² gives a*t² + b*t + c = 0.
            let a = dx * dx + dy * dy - dr * dr;
            let b = -2.0 * (x * dx + y * dy + r * dr);
            let c = x * x + y * y - r * r;
            let roots = if a.abs() < 1e-12 {
                // Treat a nearly zero quadratic coefficient as a linear equation.
                // NaN marks the cases where this solver cannot select an intersection.
                if b.abs() < 1e-12 {
                    [f64::NAN; 2]
                } else {
                    [-c / b; 2]
                }
            } else {
                let d = b * b - 4.0 * a * c;
                if d < 0.0 {
                    [f64::NAN; 2]
                } else {
                    [(-b - d.sqrt()) / (2.0 * a), (-b + d.sqrt()) / (2.0 * a)]
                }
            };
            // Discard non-real solutions and circles with negative radius. When
            // both roots survive, prefer the larger-radius circle: the smaller t
            // for shrinking radii, otherwise the larger t (also used for equal radii).
            let t = roots
                .into_iter()
                .filter(|t| t.is_finite() && r + t * dr >= 0.0)
                .reduce(|a, b| if dr < 0.0 { a.min(b) } else { a.max(b) });
            let Some(t) = t else {
                return Ok([0; 4]);
            };
            (t, positions, colors)
        }
        ShadingPaint::RasterImage { image, .. } => {
            let width = image.width.to_f64().ok_or(Error::ResourceLimit)?;
            let height = image.height.to_f64().ok_or(Error::ResourceLimit)?;
            let x = f64::from(p.x);
            let y = f64::from(p.y);
            if !(0.0..width).contains(&x) || !(0.0..height).contains(&y) {
                return Ok([0; 4]);
            }
            let ix = x.floor().to_usize().ok_or(Error::ResourceLimit)?;
            let iy = y.floor().to_usize().ok_or(Error::ResourceLimit)?;
            let offset = iy
                .checked_mul(image.width)
                .and_then(|v| v.checked_add(ix))
                .and_then(|v| v.checked_mul(4))
                .ok_or(Error::ResourceLimit)?;
            let end = offset.checked_add(4).ok_or(Error::ResourceLimit)?;
            return image
                .data
                .get(offset..end)
                .and_then(|v| v.try_into().ok())
                .ok_or(Error::InvalidInput("raster sample"));
        }
    };
    // Extend the gradient at its endpoints; duplicate-stop handling lives in interpolate.
    interpolate(t.clamp(0.0, 1.0), positions, colors)
}

/// Applies a transform and rejects arithmetic overflow or other nonfinite coordinates.
fn map_point(t: &Transform, p: Point) -> Result<Point, Error> {
    let (x, y) = t.transform_point(p.x, p.y);
    if x.is_finite() && y.is_finite() {
        Ok(Point::new(x, y))
    } else {
        Err(Error::InvalidInput("nonfinite point"))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

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

    fn linear(transform: Option<Transform>) -> ShadingPaint {
        let ColorStops { positions, colors } = stops();
        ShadingPaint::linear_gradient([0.0, 0.0, 1.0, 0.0], transform, positions, colors).unwrap()
    }

    fn radial(start_r: f32, end_x: f32, end_r: f32) -> ShadingPaint {
        let ColorStops { positions, colors } = stops();
        ShadingPaint::RadialGradient {
            start_x: 0.0,
            start_y: 0.0,
            start_r,
            end_x,
            end_y: 0.0,
            end_r,
            positions,
            colors,
            transform: Transform::identity(),
        }
    }

    #[test]
    fn linear_samples_centers_at_requested_resolution() {
        let image = rasterize_shading(&linear(None), &Rect::UNIT_RECT, [2, 1]).unwrap();
        assert_eq!((image.width, image.height), (2, 1));
        assert_eq!(image.pixel_format, PixelFormat::RGBA8888);
        assert_eq!(image.data.as_ref(), &[64, 64, 64, 255, 191, 191, 191, 255]);
        let image =
            rasterize_shading(&linear(None), &Rect::from([-1.0, 0.0, 2.0, 1.0]), [3, 1]).unwrap();
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
            &linear(Some(transform)),
            &Rect::from([5.0, 10.0, 8.0, 12.0]),
            [1, 2],
        )
        .unwrap();
        assert_eq!(image.data.as_ref(), &[64, 64, 64, 255, 191, 191, 191, 255]);
    }

    #[test]
    fn radial_handles_expanding_shrinking_and_linear_equations() {
        assert_eq!(
            sample(&radial(0.0, 0.0, 2.0), Point::new(1.0, 0.0)).unwrap(),
            [128, 128, 128, 255]
        );
        assert_eq!(
            sample(&radial(2.0, 0.0, 0.0), Point::new(0.5, 0.0)).unwrap(),
            [191, 191, 191, 255]
        );
        assert_eq!(
            sample(&radial(0.0, 1.0, 1.0), Point::new(1.0, 0.0)).unwrap(),
            [128, 128, 128, 255]
        );
        assert_eq!(
            sample(&radial(1.0, 2.0, 1.0), Point::new(0.0, 2.0)).unwrap(),
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
            },
            Rect::new(2.0, 1.0),
            Some(Transform::from_translate(3.0, 4.0)),
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
                rasterize_shading(&linear(None), &bounds, [1, 1]),
                Err(Error::InvalidInput(_))
            ));
        }
        assert!(matches!(
            rasterize_shading(&linear(None), &Rect::UNIT_RECT, [0, 1]),
            Err(Error::InvalidInput(_))
        ));
        assert!(matches!(
            rasterize_shading(&linear(None), &Rect::UNIT_RECT, [u32::MAX; 2]),
            Err(Error::ResourceLimit)
        ));
        for transform in [
            Transform::from_scale(0.0, 1.0),
            Transform::from_translate(f32::NAN, 0.0),
            Transform::from_scale(f32::INFINITY, 1.0),
        ] {
            assert!(matches!(
                rasterize_shading(
                    &ShadingPaint::LinearGradient {
                        x0: 0.0,
                        y0: 0.0,
                        x1: 1.0,
                        y1: 0.0,
                        transform,
                        positions: Arc::from([0.0, 1.0]),
                        colors: Arc::from([
                            Color::from_rgb(0.0, 0.0, 0.0),
                            Color::from_rgb(1.0, 1.0, 1.0),
                        ]),
                    },
                    &Rect::UNIT_RECT,
                    [1, 1]
                ),
                Err(Error::InvalidInput(_))
            ));
        }
        let mut paint = linear(None);
        if let ShadingPaint::LinearGradient { x1, .. } = &mut paint {
            *x1 = 0.0;
        }
        assert!(paint.validate().is_err());
        if let ShadingPaint::LinearGradient { x1, .. } = &mut paint {
            *x1 = f32::NAN;
        }
        assert!(paint.validate().is_err());
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
            let paint = ShadingPaint::RasterImage {
                image: Image {
                    data: vec![0; 3].into(),
                    width,
                    height,
                    pixel_format: PixelFormat::RGBA8888,
                },
                transform: Transform::identity(),
            };
            assert!(rasterize_shading(&paint, &Rect::UNIT_RECT, [1, 1]).is_err());
        }
        let paint = ShadingPaint::RasterImage {
            image: Image {
                data: vec![0; 4].into(),
                width: 1,
                height: 1,
                pixel_format: PixelFormat::Gray8,
            },
            transform: Transform::identity(),
        };
        assert!(paint.validate().is_err());
    }
}
