use std::sync::Arc;

use pdf_graphics::color::Color;
use pdf_object::{object_resolver::ObjectResolver, object_variant::ObjectVariant};

use crate::{
    color_space::ColorSpace, color_space_reader::parse_color_space_object, error::ColorSpaceError,
};

/// ICC profile-based color space.
///
/// Uses an embedded ICC color profile for device-independent color.
#[derive(Debug, Clone)]
pub struct ICCBasedColorSpace {
    /// Number of color components (must be 1, 3, or 4 per spec).
    pub num_components: usize,
    /// Alternate color space used as a rendering fallback.
    ///
    /// If absent, the device-equivalent for `num_components` is used instead.
    pub alternate_space: Option<Box<ColorSpace>>,
    /// Raw ICC profile bytes.
    ///
    /// ICC colour management is not yet implemented. The alternate space (or a
    /// device-equivalent) is used for rendering until ICC support is added.
    pub profile_data: Arc<[u8]>,
}

/// Parses an ICCBased color space: `[/ICCBased stream]`
///
/// The stream dictionary must contain an `/N` entry (1, 3, or 4).
/// The optional `/Alternate` entry names the fallback color space.
pub(crate) fn parse_icc_based_color_space(
    objects: &dyn ObjectResolver,
    arr: &[ObjectVariant],
    depth: usize,
) -> Result<ColorSpace, ColorSpaceError> {
    // Expected format: [/ICCBased icc-stream]
    let [_, icc_stream] = arr else {
        return Err(ColorSpaceError::InvalidColorSpace {
            description: format!("/ICCBased requires 2 elements, found {}", arr.len()),
        });
    };

    let stream = icc_stream.try_stream(objects)?;
    let num_components = stream
        .dictionary
        .get_or_err("N")?
        .try_number::<usize>(objects)?;

    // N shall be 1, 3, or 4.
    if !matches!(num_components, 1 | 3 | 4) {
        return Err(ColorSpaceError::InvalidColorSpace {
            description: format!("/ICCBased /N must be 1, 3, or 4; found {num_components}"),
        });
    }

    let alternate_space = stream
        .dictionary
        .get("Alternate")
        .map(|alt| parse_color_space_object(objects, alt, depth))
        .transpose()?
        .map(Box::new);

    let profile_data: Arc<[u8]> = stream.data()?.into_owned().into();

    Ok(ColorSpace::ICCBased(ICCBasedColorSpace {
        num_components,
        alternate_space,
        profile_data,
    }))
}

impl ICCBasedColorSpace {
    pub(crate) fn apply(&self, components: &[f32]) -> Result<Color, ColorSpaceError> {
        if components.len() != self.num_components {
            return Err(ColorSpaceError::InsufficientComponents(
                self.num_components,
                components.len(),
            ));
        }
        // ICC colour management is not yet implemented.
        // Use the alternate space if present; otherwise use the device-equivalent.
        if let Some(alt) = &self.alternate_space {
            return alt.apply(components);
        }
        match (self.num_components, components) {
            (1, [g]) => Ok(Color::from_gray(*g)),
            (3, [r, g, b]) => Ok(Color::from_rgb(*r, *g, *b)),
            (4, [c, m, y, k]) => Ok(Color::from_cmyk(*c, *m, *y, *k)),
            (n, _) => Err(ColorSpaceError::Unsupported(format!(
                "ICCBased with {n} components has no alternate space"
            ))),
        }
    }
}
