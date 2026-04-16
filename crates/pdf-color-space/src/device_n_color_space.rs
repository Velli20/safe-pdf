use pdf_graphics::color::Color;
use pdf_object::{object_resolver::ObjectResolver, object_variant::ObjectVariant};

use crate::{
    color_space::ColorSpace, color_space_reader::parse_color_space_object, error::ColorSpaceError,
};

/// DeviceN color space.
///
/// Represents one or more named colorants with a fallback alternate color space
/// and a tint transform function. DeviceN generalises [`Separation`] to handle
/// multiple colorants simultaneously.
///
/// [`Separation`]: crate::separation_color_space::SeparationColorSpace
///
/// # Rendering limitation
///
/// The tint transform for DeviceN accepts `n` inputs (one per colorant), but the
/// current [`Function`] API only supports single-input evaluation. Calling
/// [`apply`] therefore returns [`ColorSpaceError::Unsupported`] until multi-input
/// function evaluation is implemented.
///
/// [`Function`]: pdf_function::function::Function
/// [`apply`]: DeviceNColorSpace::apply
#[derive(Debug, Clone)]
pub struct DeviceNColorSpace {
    /// Ordered list of colorant names (e.g., `["Cyan", "Magenta"]`).
    pub names: Vec<String>,
    /// Fallback color space used when the colorants are not available.
    pub alternate_space: Box<ColorSpace>,
}

/// Parses a DeviceN color space: `[/DeviceN names alternateSpace tintTransform]`
///
/// An optional fifth element (attributes dictionary) is accepted and ignored.
/// The tint transform is also ignored for now; see the rendering limitation in
/// [`DeviceNColorSpace`].
pub(crate) fn parse_device_n_color_space(
    objects: &dyn ObjectResolver,
    arr: &[ObjectVariant],
    depth: usize,
) -> Result<ColorSpace, ColorSpaceError> {
    let [_, names_obj, alt_obj, _, _] = arr else {
        return Err(ColorSpaceError::InvalidColorSpace {
            description: format!("/DeviceN requires at least 4 elements, found {}", arr.len()),
        });
    };

    let names = names_obj
        .try_array(objects)?
        .iter()
        .map(|n| n.try_str(objects).map(|s| s.into_owned()))
        .collect::<Result<Vec<_>, _>>()?;

    let alternate_space = parse_color_space_object(objects, alt_obj, depth)?;

    Ok(ColorSpace::DeviceN(DeviceNColorSpace {
        names,
        alternate_space: Box::new(alternate_space),
    }))
}

impl DeviceNColorSpace {
    /// Returns the number of color components (one per named colorant).
    #[must_use]
    pub fn num_components(&self) -> usize {
        self.names.len()
    }

    /// Not yet supported — requires multi-input function evaluation.
    ///
    /// See the rendering limitation documented on [`DeviceNColorSpace`].
    pub(crate) fn apply(&self, _components: &[f32]) -> Result<Color, ColorSpaceError> {
        Err(ColorSpaceError::Unsupported(format!(
            "DeviceN with {} colorant(s) requires multi-input function support",
            self.names.len()
        )))
    }
}
