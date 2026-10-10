//! Color-stop sampling helpers for PDF shadings.

use std::sync::Arc;

use pdf_color_space::color_space::ColorSpace;
use pdf_function::function::{Function, FunctionImpl};
use pdf_graphics::{color::Color, interval::Interval};

use crate::error::{PdfShadingError, ShadingRasterError};
use num_traits::ToPrimitive;

/// A collection of sampled colors and normalized positions for gradient-style shadings.
#[derive(Debug, Clone, Default)]
pub struct ColorStops {
    /// The colors at each stop position.
    pub colors: Arc<[Color]>,
    /// The normalized stop positions in the inclusive `0.0..=1.0` range.
    pub positions: Arc<[f32]>,
}

impl ColorStops {
    /// Number of color stops sampled when converting a function to gradient stops.
    const DEFAULT_NUM_COLOR_STOPS: u16 = 16;

    /// Samples a shading function into discrete color stops for backend gradients.
    pub fn from_function(
        function: &Function,
        color_space: &ColorSpace,
    ) -> Result<Self, PdfShadingError> {
        let domain = function.domain().unwrap_or(Interval::UNIT);
        Self::from_function_domain(function, color_space, domain)
    }

    /// Samples a shading function across an explicit shading domain.
    pub fn from_function_domain(
        function: &Function,
        color_space: &ColorSpace,
        domain: Interval,
    ) -> Result<Self, PdfShadingError> {
        if !domain.is_increasing() {
            return Err(PdfShadingError::UnsupportedFeature(
                "invalid shading domain".to_string(),
            ));
        }

        let capacity = usize::from(Self::DEFAULT_NUM_COLOR_STOPS);
        let mut positions = Vec::with_capacity(capacity);
        let mut colors = Vec::with_capacity(capacity);

        let denominator = f32::from(Self::DEFAULT_NUM_COLOR_STOPS - 1);
        for i in 0..Self::DEFAULT_NUM_COLOR_STOPS {
            let t = f32::from(i) / denominator;
            let components = function.interpolate(&[domain.lerp(t)])?;
            let color = color_space.apply(&components)?;

            positions.push(t);
            colors.push(color);
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
}

impl TryFrom<&Function> for ColorStops {
    type Error = PdfShadingError;

    /// Samples a shading function into color stops using `DeviceRGB`.
    fn try_from(function: &Function) -> Result<Self, Self::Error> {
        Self::from_function(function, &ColorSpace::DeviceRGB)
    }
}

/// Validates borrowed backend gradient stops before sampling.
pub(crate) fn validate_stops(
    positions: &[f32],
    colors: &[Color],
) -> Result<(), ShadingRasterError> {
    if positions.len() != colors.len()
        || positions.len() < 2
        || positions
            .iter()
            .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
        || positions.windows(2).any(|v| matches!(v,[a,b] if a>b))
        || colors
            .iter()
            .any(|c| [c.r, c.g, c.b, c.a].iter().any(|v| !v.is_finite()))
    {
        return Err(ShadingRasterError::InvalidInput(
            "gradient stops or coordinates",
        ));
    }
    Ok(())
}

// The stop slices must have passed validate_stops before interpolation.
pub(crate) fn interpolate(
    t: f64,
    positions: &[f32],
    colors: &[Color],
) -> Result<[u8; 4], ShadingRasterError> {
    let Some((first_pos, first)) = positions.first().zip(colors.first()) else {
        return Err(ShadingRasterError::InvalidInput("empty stops"));
    };
    if t < f64::from(*first_pos) {
        return Ok(first.to_rgba8());
    }
    for (pair, colors) in positions.windows(2).zip(colors.windows(2)) {
        if let ([a, b], [ca, cb]) = (pair, colors)
            && t < f64::from(*b)
        {
            let f = ((t - f64::from(*a)) / f64::from(*b - *a))
                .to_f32()
                .ok_or(ShadingRasterError::InvalidInput("stop interpolation"))?;
            return Ok(Color::from_rgba(
                ca.r + (cb.r - ca.r) * f,
                ca.g + (cb.g - ca.g) * f,
                ca.b + (cb.b - ca.b) * f,
                ca.a + (cb.a - ca.a) * f,
            )
            .to_rgba8());
        }
    }
    colors
        .last()
        .map(|c| c.to_rgba8())
        .ok_or(ShadingRasterError::InvalidInput("empty stops"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_function::{exponential_interpolation::ExponentialFunction, function::Function};

    #[test]
    fn explicit_domain_includes_both_endpoints() {
        let function = Function::Exponential(ExponentialFunction::new(
            vec![0.0; 3],
            vec![1.0; 3],
            1.0,
            Interval::new(0.0, 10.0),
        ));
        let stops = ColorStops::from_function_domain(
            &function,
            &ColorSpace::DeviceRGB,
            Interval::new(2.0, 8.0),
        )
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
        assert_eq!(
            interpolate(0.5, &[0.0, 0.5, 0.5], &colors).unwrap(),
            [0, 0, 255, 255]
        );
    }
    #[test]
    fn rejects_invalid_stops() {
        let colors = [
            Color::from_rgb(0.0, 0.0, 0.0),
            Color::from_rgb(1.0, 1.0, 1.0),
        ];
        for positions in [
            vec![],
            vec![0.0],
            vec![0.0, 0.5, 1.0],
            vec![1.0, 0.0],
            vec![-0.1, 1.0],
            vec![0.0, 1.1],
            vec![0.0, f32::NAN],
        ] {
            assert!(validate_stops(&positions, &colors).is_err());
        }
        let mut invalid = colors;
        if let Some(color) = invalid.first_mut() {
            color.a = f32::NAN;
        }
        assert!(validate_stops(&[0.0, 1.0], &invalid).is_err());
        assert!(validate_stops(&[0.5, 0.5], &colors).is_ok());
    }

    #[test]
    fn interpolates_straight_alpha_and_extends_endpoint_colors() {
        let colors = [
            Color::from_rgba(1.0, 0.0, 0.0, 0.0),
            Color::from_rgba(0.0, 0.0, 1.0, 1.0),
        ];
        assert_eq!(
            interpolate(0.5, &[0.25, 0.75], &colors).unwrap(),
            [128, 0, 128, 128]
        );
        assert_eq!(
            interpolate(0.0, &[0.25, 0.75], &colors).unwrap(),
            [255, 0, 0, 0]
        );
        assert_eq!(
            interpolate(1.0, &[0.25, 0.75], &colors).unwrap(),
            [0, 0, 255, 255]
        );
    }
}
