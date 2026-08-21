use pdf_function::function::{Function, FunctionImpl};
use pdf_graphics::color::Color;
use pdf_object::{object_resolver::ObjectResolver, object_variant::ObjectVariant};

use crate::{
    color_space::ColorSpace, color_space_reader::parse_color_space_object, error::ColorSpaceError,
};

/// Separation color space.
///
/// Represents a single colorant that is not one of the standard device colorants.
/// Includes a fallback `alternate_space` and a `tint_transform` function to convert tint values.
#[derive(Debug, Clone)]
pub struct SeparationColorSpace {
    /// The name of the colorant (e.g., `/All`, `/None`, or a custom name).
    pub name: Vec<u8>,
    /// The alternate color space to use if the separation is not supported.
    pub alternate_space: Box<ColorSpace>,
    /// The tint transform function (transforms tint 0.0-1.0 to alternate space).
    /// Typically a Function object (Dictionary or Stream).
    pub tint_transform: Function,
}

/// Parses a Separation color space: `[/Separation name alternateSpace tintTransform]`
pub(crate) fn parse_separation_color_space(
    objects: &dyn ObjectResolver,
    arr: &[ObjectVariant],
    depth: usize,
) -> Result<ColorSpace, ColorSpaceError> {
    // Expected format: [/Separation name alternateSpace tintTransform]
    let [_, name, alternate_space, tint_transform] = arr else {
        return Err(ColorSpaceError::InvalidColorSpace {
            description: format!("/Separation requires 4 elements, found {}", arr.len()),
        });
    };

    let name = Vec::from(name.try_bytes(objects)?);
    let alternate_space = parse_color_space_object(objects, alternate_space, depth)?;
    let tint_transform = Function::parse(objects.resolve_object(tint_transform)?, objects)?;

    Ok(ColorSpace::Separation(SeparationColorSpace {
        name,
        alternate_space: Box::new(alternate_space),
        tint_transform,
    }))
}

impl SeparationColorSpace {
    pub fn apply(&self, components: &[f32]) -> Result<Color, ColorSpaceError> {
        // If the name is "None", it represents the absence of all colorants.
        // Produce fully transparent output regardless of tint value.
        if self.name == b"None" {
            return Ok(Color::from_rgba(0.0, 0.0, 0.0, 0.0));
        }

        let tint = components
            .first()
            .copied()
            .ok_or(ColorSpaceError::InsufficientComponents(1, components.len()))?;
        let alt = self.tint_transform.apply(&[tint]).map_err(|e| {
            ColorSpaceError::Unsupported(format!("Separation tint transform failed: {e}"))
        })?;

        self.alternate_space.apply(&alt)
    }
}
