use crate::{
    color_space::ColorSpace,
    functions::{Function, FunctionImpl, FunctionInterpolationError},
};
use pdf_graphics::color::Color;

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
    /// Samples a shading function to create discrete color stops.
    ///
    /// The function output is interpreted according to the provided `color_space`.
    pub fn from_function(
        function: &Function,
        color_space: &ColorSpace,
    ) -> Result<Self, FunctionInterpolationError> {
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
            let color = function
                .interpolate(x)
                .and_then(|components| Self::components_to_color(color_space, &components))?;

            positions.push(t);
            colors.push(color);
        }

        Ok(Self { colors, positions })
    }
}

impl TryFrom<&Function> for ColorStops {
    type Error = FunctionInterpolationError;

    /// Samples a shading function to create discrete color stops.
    ///
    /// This method evaluates the function at `num_stops` evenly-spaced positions
    /// across its domain, converting the continuous function into a series of
    /// discrete color values suitable for gradient rendering.
    ///
    /// # Arguments
    ///
    /// * `function` - The shading function to sample.
    ///
    /// # Returns
    ///
    /// A `ColorStops` instance containing the sampled colors and their positions.
    ///
    /// # Note
    ///
    /// If function evaluation fails at any point, a default black color is used.
    /// Color components beyond the first three (RGB) are ignored.
    fn try_from(function: &Function) -> Result<Self, Self::Error> {
        // Backwards-compatible behavior: interpret output as RGB.
        Self::from_function(function, &ColorSpace::DeviceRGB)
    }
}

impl ColorStops {
    /// Number of color stops to sample when converting a function to discrete gradient stops.
    /// Higher values produce smoother gradients but increase memory usage.
    const DEFAULT_NUM_COLOR_STOPS: u16 = 16;

    /// Converts a slice of color components to an RGBA `Color`.
    ///
    /// The PDF function output dimensionality depends on the shading `/ColorSpace`.
    /// This helper performs a best-effort conversion into the renderer's RGB(A)
    /// `Color` representation.
    #[inline]
    fn components_to_color(
        color_space: &ColorSpace,
        components: &[f32],
    ) -> Result<Color, FunctionInterpolationError> {
        match color_space {
            ColorSpace::DeviceRGB => {
                let [r, g, b] = components else {
                    return Err(FunctionInterpolationError::InsufficientColorComponents {
                        required: 3,
                        returned: components.len(),
                    });
                };
                Ok(Color::from_rgb(*r, *g, *b))
            }
            ColorSpace::DeviceGray => {
                let Some(gray) = components.first().copied() else {
                    return Err(FunctionInterpolationError::InsufficientColorComponents {
                        required: 1,
                        returned: components.len(),
                    });
                };
                Ok(Color::from_gray(gray))
            }
            ColorSpace::DeviceCMYK => {
                let [c, m, y, k] = components else {
                    return Err(FunctionInterpolationError::InsufficientColorComponents {
                        required: 4,
                        returned: components.len(),
                    });
                };
                Ok(Color::from_cmyk(*c, *m, *y, *k))
            }
            ColorSpace::ICCBased { num_components } => match num_components {
                1 => Self::components_to_color(&ColorSpace::DeviceGray, components),
                3 => Self::components_to_color(&ColorSpace::DeviceRGB, components),
                4 => Self::components_to_color(&ColorSpace::DeviceCMYK, components),
                _ => Self::components_to_color(&ColorSpace::DeviceRGB, components),
            },
            ColorSpace::Indexed { .. } => {
                Err(FunctionInterpolationError::IndexedColorSpaceUnsupported)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn device_gray_maps_first_component() -> Result<(), FunctionInterpolationError> {
        let c = ColorStops::components_to_color(&ColorSpace::DeviceGray, &[0.25])?;
        assert!(approx(c.r, 0.25) && approx(c.g, 0.25) && approx(c.b, 0.25));
        Ok(())
    }

    #[test]
    fn device_cmyk_converts_to_rgb() -> Result<(), FunctionInterpolationError> {
        // Magenta: C=0, M=1, Y=0, K=0 -> RGB=(1,0,1)
        let c = ColorStops::components_to_color(&ColorSpace::DeviceCMYK, &[0.0, 1.0, 0.0, 0.0])?;
        assert!(approx(c.r, 1.0) && approx(c.g, 0.0) && approx(c.b, 1.0));
        Ok(())
    }
}
