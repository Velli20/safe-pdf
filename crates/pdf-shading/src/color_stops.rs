//! Color-stop sampling helpers for PDF shadings.

use std::sync::Arc;

use num_traits::ToPrimitive;
use pdf_color_space::color_space::ColorSpace;
use pdf_function::function::Function;
use pdf_graphics::{color::Color, interval::Interval};
use pdf_image::ImageRasterError;

use crate::error::PdfShadingError;

/// A collection of sampled colors and normalized positions for gradient-style shadings.
#[derive(Debug, Clone, PartialEq)]
pub struct ColorStops {
    /// The colors at each stop position.
    pub colors: Arc<[Color]>,
    /// The normalized stop positions in the inclusive `0.0..=1.0` range.
    pub positions: Arc<[f32]>,
}

impl ColorStops {
    /// Number of color stops sampled when converting a function to gradient stops.
    const DEFAULT_NUM_COLOR_STOPS: u16 = 16;

    /// Samples a shading function across a shading domain into discrete color stops.
    ///
    /// The function is sampled at evenly spaced positions and on both sides of each
    /// breakpoint inside the domain, so a stitching function keeps every segment
    /// however many it has. Both samples of a breakpoint share its position, which
    /// makes the handover a hard edge.
    pub fn from_function(
        function: &Function,
        color_space: &ColorSpace,
        domain: Interval,
    ) -> Result<Self, PdfShadingError> {
        if !domain.is_increasing() {
            return Err(PdfShadingError::InvalidDomain(domain));
        }

        let breakpoints = function.breakpoints();
        let capacity = usize::from(Self::DEFAULT_NUM_COLOR_STOPS)
            .saturating_add(breakpoints.len().saturating_mul(2));
        let mut positions = Vec::with_capacity(capacity);
        let mut colors = Vec::with_capacity(capacity);
        let mut sample = |t: f32, x: f32| -> Result<(), PdfShadingError> {
            let components = function.apply(&[x])?;
            colors.push(color_space.apply(&components)?);
            positions.push(t);
            Ok(())
        };

        let denominator = f32::from(Self::DEFAULT_NUM_COLOR_STOPS - 1);
        let mut grid = (0..Self::DEFAULT_NUM_COLOR_STOPS)
            .map(|i| f32::from(i) / denominator)
            .peekable();
        // Malformed /Bounds may be out of order; breakpoints behind the last one are
        // skipped so the positions stay sorted.
        let mut last = 0.0;
        for &b in breakpoints {
            let Some(t) = domain
                .normalize(b)
                .filter(|_| domain.min < b && b < domain.max)
                .filter(|t| *t >= last)
            else {
                continue;
            };
            while let Some(g) = grid.next_if(|g| *g < t) {
                sample(g, domain.lerp(g))?;
            }
            // An input exactly on a bound selects the next segment, so the segment
            // ending there is sampled just below it.
            sample(t, b.next_down())?;
            sample(t, b)?;
            last = t;
        }
        for g in grid {
            sample(g, domain.lerp(g))?;
        }

        Ok(Self {
            colors: colors.into(),
            positions: positions.into(),
        })
    }

    /// Returns stops that paint `outside` past each end that does not extend.
    ///
    /// Each such end gets a hard stop holding `outside`. Backends pad past the end
    /// stops, so the area beyond that end takes `outside` instead of the end color.
    pub fn bounded(&self, extend: [bool; 2], outside: Color) -> Self {
        let [extend_start, extend_end] = extend;
        if extend_start && extend_end {
            return self.clone();
        }
        let start = (!extend_start).then_some((0.0, outside));
        let end = (!extend_end).then_some((1.0, outside));
        let (positions, colors): (Vec<f32>, Vec<Color>) = start
            .into_iter()
            .chain(
                self.positions
                    .iter()
                    .copied()
                    .zip(self.colors.iter().copied()),
            )
            .chain(end)
            .unzip();
        Self {
            colors: colors.into(),
            positions: positions.into(),
        }
    }

    /// Returns the stops with every color channel clamped to `[0.0, 1.0]`.
    pub(crate) fn clamped(&self) -> Self {
        Self {
            colors: self.colors.iter().map(|color| color.clamped()).collect(),
            positions: Arc::clone(&self.positions),
        }
    }

    /// Checks that there are at least two stops, with valid colors and ordered positions
    /// between zero and one.
    pub(crate) fn validate(&self) -> Result<(), ImageRasterError> {
        let Self { colors, positions } = self;
        if positions.len() != colors.len()
            || positions.len() < 2
            || !positions.is_sorted()
            || !positions
                .iter()
                .all(|position| (0.0..=1.0).contains(position))
            || !colors.iter().all(Color::is_valid)
        {
            return Err(ImageRasterError::InvalidInput(
                "gradient stops or coordinates",
            ));
        }
        Ok(())
    }

    /// Returns the color at the gradient parameter `t`, padding past the end stops.
    ///
    /// Where stops share a position, the last of them applies from it.
    pub(crate) fn color_at(&self, t: f64) -> Color {
        let end = self
            .positions
            .partition_point(|position| f64::from(*position) <= t);
        let start = end.checked_sub(1).and_then(|start| self.stop(start));
        match (start, self.stop(end)) {
            (Some((start, from)), Some((end, to))) => {
                let fraction = (t - f64::from(start)) / f64::from(end - start);
                from.lerp(to, fraction.to_f32().unwrap_or_default())
            }
            (Some((_, color)), None) | (None, Some((_, color))) => color,
            (None, None) => Color::TRANSPARENT,
        }
    }

    /// Returns the position and color of the stop at `index`.
    fn stop(&self, index: usize) -> Option<(f32, Color)> {
        Some((*self.positions.get(index)?, *self.colors.get(index)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_function::{exponential_interpolation::ExponentialFunction, function::Function};

    #[test]
    fn explicit_domain_includes_both_endpoints() {
        let function = Function::Exponential(ExponentialFunction::new(
            vec![0.0; 3],
            vec![0.1; 3],
            1.0,
            Interval::new(0.0, 10.0),
        ));
        let stops =
            ColorStops::from_function(&function, &ColorSpace::DeviceRGB, Interval::new(2.0, 8.0))
                .unwrap();

        assert_eq!(stops.positions.first(), Some(&0.0));
        assert_eq!(stops.positions.last(), Some(&1.0));
        assert_eq!(stops.colors.first(), Some(&Color::from_rgb(0.2, 0.2, 0.2)));
        assert_eq!(stops.colors.last(), Some(&Color::from_rgb(0.8, 0.8, 0.8)));
    }

    #[test]
    fn duplicate_stops_choose_last_color_at_boundary() {
        let colors = [
            Color::from_rgb(1.0, 0.0, 0.0),
            Color::from_rgb(0.0, 1.0, 0.0),
            Color::from_rgb(0.0, 0.0, 1.0),
        ];
        let stops = ColorStops {
            colors: colors.into(),
            positions: [0.0, 0.5, 0.5].into(),
        };
        assert_eq!(stops.color_at(0.5).to_rgba8(), [0, 0, 255, 255]);
    }

    #[test]
    fn rejects_invalid_stops() {
        let colors = [
            Color::from_rgb(0.0, 0.0, 0.0),
            Color::from_rgb(1.0, 1.0, 1.0),
        ];
        let stops = |positions: &[f32], colors: [Color; 2]| ColorStops {
            colors: colors.into(),
            positions: positions.into(),
        };
        for positions in [
            vec![],
            vec![0.0],
            vec![0.0, 0.5, 1.0],
            vec![1.0, 0.0],
            vec![-0.1, 1.0],
            vec![0.0, 1.1],
            vec![0.0, f32::NAN],
        ] {
            assert!(stops(&positions, colors).validate().is_err());
        }
        let mut invalid = colors;
        if let Some(color) = invalid.first_mut() {
            color.a = f32::NAN;
        }
        assert!(stops(&[0.0, 1.0], invalid).validate().is_err());
        assert!(stops(&[0.5, 0.5], colors).validate().is_ok());
    }

    #[test]
    fn interpolates_straight_alpha_and_extends_endpoint_colors() {
        let colors = [
            Color::from_rgba(1.0, 0.0, 0.0, 0.0),
            Color::from_rgba(0.0, 0.0, 1.0, 1.0),
        ];
        let stops = ColorStops {
            colors: colors.into(),
            positions: [0.25, 0.75].into(),
        };
        assert_eq!(stops.color_at(0.5).to_rgba8(), [128, 0, 128, 128]);
        assert_eq!(stops.color_at(0.0).to_rgba8(), [255, 0, 0, 0]);
        assert_eq!(stops.color_at(1.0).to_rgba8(), [0, 0, 255, 255]);
    }
}
