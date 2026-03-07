use pdf_color_space::color_space::ColorSpace;
use pdf_function::function::{Function, FunctionImpl};
use pdf_graphics::color::Color;

use crate::pages::PdfPagesError;

/// Default domain range for shading functions when not explicitly specified.
const DEFAULT_DOMAIN: [f32; 2] = [0.0, 1.0];

/// A collection of color stops representing a sampled gradient.
///
/// Color stops are used to convert continuous PDF shading functions into
/// discrete gradient representations suitable for rendering backends.
#[derive(Debug, Clone, Default)]
pub struct ColorStops {
    /// The colors at each stop position.
    pub colors: Vec<Color>,
    /// Normalized positions (0.0 to 1.0) for each color stop.
    /// Must have the same length as `colors`.
    pub positions: Vec<f32>,
}

impl ColorStops {
    /// Number of color stops to sample when converting a function to discrete gradient stops.
    /// Higher values produce smoother gradients but increase memory usage.
    const DEFAULT_NUM_COLOR_STOPS: u16 = 16;

    /// Samples a shading function to create discrete color stops.
    ///
    /// The function output is interpreted according to the provided `color_space`.
    pub fn from_function(
        function: &Function,
        color_space: &ColorSpace,
    ) -> Result<Self, PdfPagesError> {
        let domain = function.domain().unwrap_or(DEFAULT_DOMAIN);
        let domain_range = domain[1] - domain[0];

        // Pre-allocate vectors with known capacity for efficiency.
        let capacity = usize::from(Self::DEFAULT_NUM_COLOR_STOPS);
        let mut positions = Vec::with_capacity(capacity);
        let mut colors = Vec::with_capacity(capacity);

        for i in 0..Self::DEFAULT_NUM_COLOR_STOPS {
            // Calculate normalized position (0.0 to 1.0).
            let t = f32::from(i) / f32::from(Self::DEFAULT_NUM_COLOR_STOPS);

            // Map to function domain.
            let x = domain[0] + t * domain_range;

            // Evaluate function; propagate errors to the caller.
            let components = function.interpolate(&[x])?;
            let color = color_space.apply(&components)?;

            positions.push(t);
            colors.push(color);
        }

        Ok(Self { colors, positions })
    }
}

impl TryFrom<&Function> for ColorStops {
    type Error = PdfPagesError;

    /// Samples a shading function to create discrete color stops.
    ///
    /// This method evaluates the function at `num_stops` evenly-spaced positions
    /// across its domain, converting the continuous function into a series of
    /// discrete color values suitable for gradient rendering.
    ///
    /// # Parameters
    ///
    /// - `function`: The shading function to sample.
    ///
    /// # Returns
    ///
    /// A `ColorStops` instance containing the sampled colors and their positions.
    fn try_from(function: &Function) -> Result<Self, Self::Error> {
        Self::from_function(function, &ColorSpace::DeviceRGB)
    }
}
