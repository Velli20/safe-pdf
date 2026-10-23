use std::sync::Arc;

use pdf_function::function::Function;
use pdf_graphics::color::Color;
use pdf_object_reader::{FromPdfObject, ObjectAccess, ObjectContext, ReadResult};

use crate::{color_space::ColorSpace, error::ColorSpaceError};

/// Separation color space.
///
/// Represents a single colorant that is not one of the standard device colorants.
/// Includes a fallback `alternate_space` and a `tint_transform` function to convert tint values.
#[derive(Debug, Clone)]
pub struct SeparationColorSpace {
    /// The name of the colorant (e.g., `/All`, `/None`, or a custom name).
    pub name: Arc<[u8]>,
    /// The alternate color space to use if the separation is not supported.
    pub alternate_space: Box<ColorSpace>,
    /// The tint transform function (transforms tint 0.0-1.0 to alternate space).
    /// Typically a Function object (Dictionary or Stream).
    pub tint_transform: Function,
}

/// Reads a Separation color space: `[/Separation name alternateSpace tintTransform]`
impl FromPdfObject for SeparationColorSpace {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut array = context.array()?;
        let [_, _, _, _] = array.array() else {
            return Err(ColorSpaceError::InvalidColorSpace {
                description: format!(
                    "/Separation requires 4 elements, found {}",
                    array.array().len()
                ),
            }
            .into());
        };

        Ok(Self {
            name: array.at(1)?,
            alternate_space: Box::new(array.at(2)?),
            tint_transform: array.at(3)?,
        })
    }
}

impl SeparationColorSpace {
    pub fn apply(&self, components: &[f32]) -> Result<Color, ColorSpaceError> {
        // If the name is "None", it represents the absence of all colorants.
        // Produce fully transparent output regardless of tint value.
        if &*self.name == b"None" {
            return Ok(Color::TRANSPARENT);
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
