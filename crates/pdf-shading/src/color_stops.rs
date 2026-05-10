//! Color-stop sampling helpers for PDF shadings.

use pdf_color_space::color_space::ColorSpace;
use pdf_function::function::{Function, FunctionImpl};
use pdf_graphics::color::Color;

use crate::error::PdfShadingError;

/// Default domain range for shading functions when not explicitly specified.
const DEFAULT_DOMAIN: [f32; 2] = [0.0, 1.0];

/// A collection of sampled colors and normalized positions for gradient-style shadings.
#[derive(Debug, Clone, Default)]
pub struct ColorStops {
    /// The colors at each stop position.
    pub colors: Vec<Color>,
    /// The normalized stop positions in the inclusive `0.0..=1.0` range.
    pub positions: Vec<f32>,
}

impl ColorStops {
    /// Number of color stops sampled when converting a function to gradient stops.
    const DEFAULT_NUM_COLOR_STOPS: u16 = 16;

    /// Samples a shading function into discrete color stops for backend gradients.
    pub fn from_function(
        function: &Function,
        color_space: &ColorSpace,
    ) -> Result<Self, PdfShadingError> {
        let domain = match function.domain() {
            Some(values) => values,
            None => DEFAULT_DOMAIN,
        };
        let domain_range = domain[1] - domain[0];

        let capacity = usize::from(Self::DEFAULT_NUM_COLOR_STOPS);
        let mut positions = Vec::with_capacity(capacity);
        let mut colors = Vec::with_capacity(capacity);

        for i in 0..Self::DEFAULT_NUM_COLOR_STOPS {
            let t = f32::from(i) / f32::from(Self::DEFAULT_NUM_COLOR_STOPS);
            let x = domain[0] + t * domain_range;
            let components = function.interpolate(&[x])?;
            let color = color_space.apply(&components)?;

            positions.push(t);
            colors.push(color);
        }

        Ok(Self { colors, positions })
    }
}

impl TryFrom<&Function> for ColorStops {
    type Error = PdfShadingError;

    /// Samples a shading function into color stops using `DeviceRGB`.
    fn try_from(function: &Function) -> Result<Self, Self::Error> {
        Self::from_function(function, &ColorSpace::DeviceRGB)
    }
}
