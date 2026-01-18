use pdf_object::{
    ObjectVariant, dictionary::Dictionary, error::ObjectError, object_collection::ObjectCollection,
    traits::FromDictionary,
};
use thiserror::Error;

use crate::functions::{Function, FunctionImpl, FunctionReadError};

/// Maximum nesting depth for color space definitions.
///
/// Prevents stack overflow from maliciously crafted PDFs with deeply nested
/// color spaces (e.g., Indexed within Indexed within Indexed...).
const MAX_COLOR_SPACE_DEPTH: usize = 8;

/// Errors that can occur when parsing PDF color spaces.
#[derive(Debug, Error)]
pub enum ColorSpaceError {
    /// An error occurred while resolving PDF objects.
    #[error("Failed to resolve PDF object: {0}")]
    ObjectError(#[from] ObjectError),
    /// The color space definition is invalid or unsupported.
    #[error("Invalid or unsupported ColorSpace: {description}")]
    InvalidColorSpace { description: String },
    /// Error parsing function for Separation/DeviceN color spaces.
    #[error("Function parsing error: {0}")]
    FunctionError(#[from] FunctionReadError),
}

/// Represents a PDF color space.
///
/// Color spaces define how color values are interpreted and rendered.
/// PDF supports several families of color spaces:
///
/// - **Device color spaces**: Direct color specification (`DeviceGray`, `DeviceRGB`, `DeviceCMYK`)
/// - **CIE-based color spaces**: Device-independent color (e.g., `ICCBased`)
/// - **Special color spaces**: Indexed, Pattern, Separation, DeviceN
#[derive(Debug, Clone)]
pub enum ColorSpace {
    /// Grayscale color space with a single component (0.0 = black, 1.0 = white).
    DeviceGray,
    /// RGB color space with three components (Red, Green, Blue).
    DeviceRGB,
    /// CMYK color space with four components (Cyan, Magenta, Yellow, Black).
    DeviceCMYK,
    /// Indexed (palette-based) color space.
    ///
    /// Maps integer indices to colors in a base color space via a lookup table.
    /// Commonly used for images with a limited color palette.
    Indexed {
        /// The underlying color space for palette entries.
        base: Box<ColorSpace>,
        /// Maximum valid index value (0 to 255). The palette contains `hival + 1` entries.
        hival: u8,
        /// Raw lookup table bytes. Each entry contains `base.num_color_components()` bytes.
        lookup: Vec<u8>,
    },
    /// ICC profile-based color space.
    ///
    /// Uses an embedded ICC color profile for device-independent color.
    ICCBased {
        /// Number of color components (1, 3, or 4 depending on profile).
        num_components: usize,
    },
    /// Separation color space.
    ///
    /// Represents a single colorant (spot color) that is not one of the standard device colorants.
    /// Includes a fallback `alternate_space` and a `tint_transform` function to convert tint values.
    Separation {
        /// The name of the colorant (e.g., `/All`, `/None`, or a custom name).
        name: String,
        /// The alternate color space to use if the separation is not supported.
        alternate_space: Box<ColorSpace>,
        /// The tint transform function (transforms tint 0.0-1.0 to alternate space).
        /// Typically a Function object (Dictionary or Stream).
        tint_transform: Function,
    },
}

impl ColorSpace {
    /// Returns the number of color components for this color space.
    ///
    /// - `DeviceGray`: 1
    /// - `DeviceRGB`: 3
    /// - `DeviceCMYK`: 4
    /// - `Indexed`: Same as the base color space (for decoded colors)
    /// - `ICCBased`: Determined by the ICC profile (typically 1, 3, or 4)
    #[must_use]
    pub const fn num_color_components(&self) -> usize {
        match self {
            Self::DeviceGray => 1,
            Self::DeviceRGB => 3,
            Self::DeviceCMYK => 4,
            Self::Indexed { base, .. } => base.num_color_components(),
            Self::ICCBased { num_components } => *num_components,
            Self::Separation { .. } => 1,
        }
    }

    /// Returns the number of bits per pixel given the bits per component.
    ///
    /// Calculated as `bits_per_component * num_color_components()`.
    /// Uses saturating multiplication to prevent overflow.
    #[must_use]
    pub const fn bits_per_pixel(&self, bits_per_component: usize) -> usize {
        bits_per_component.saturating_mul(self.num_color_components())
    }

    /// Returns `true` if this is a device-dependent color space.
    #[must_use]
    pub const fn is_device_space(&self) -> bool {
        matches!(self, Self::DeviceGray | Self::DeviceRGB | Self::DeviceCMYK)
    }
}

impl FromDictionary for ColorSpace {
    const KEY: &'static str = "ColorSpace";
    type ResultType = Option<ColorSpace>;
    type ErrorType = ColorSpaceError;

    fn from_dictionary(
        dictionary: &Dictionary,
        objects: &ObjectCollection,
    ) -> Result<Self::ResultType, Self::ErrorType> {
        let Some(color_space_obj) = dictionary.get(Self::KEY) else {
            return Ok(None);
        };

        let color_space_obj = objects.resolve_object(color_space_obj)?;
        parse_color_space_object(objects, color_space_obj, 0).map(Some)
    }
}

/// Parses a color space from a PDF object.
///
/// Color spaces can be specified as:
/// - A name (e.g., `/DeviceRGB`)
/// - An array (e.g., `[/Indexed /DeviceRGB 255 <lookup data>]`)
fn parse_color_space_object(
    objects: &ObjectCollection,
    obj: &ObjectVariant,
    depth: usize,
) -> Result<ColorSpace, ColorSpaceError> {
    // Guard against deeply-nested color spaces.
    if depth >= MAX_COLOR_SPACE_DEPTH {
        return Err(ColorSpaceError::InvalidColorSpace {
            description: "color space nesting exceeds maximum depth".into(),
        });
    }

    match objects.resolve_object(obj)? {
        ObjectVariant::Array(arr) => {
            parse_color_space_array(objects, arr.as_slice(), depth.saturating_add(1))
        }
        other => parse_color_space_name(other),
    }
}

/// Parses a simple named color space (e.g., `/DeviceRGB`).
fn parse_color_space_name(obj: &ObjectVariant) -> Result<ColorSpace, ColorSpaceError> {
    let name = obj.try_str()?;

    match name.as_ref() {
        "DeviceRGB" => Ok(ColorSpace::DeviceRGB),
        "DeviceCMYK" => Ok(ColorSpace::DeviceCMYK),
        "DeviceGray" => Ok(ColorSpace::DeviceGray),
        unknown => Err(ColorSpaceError::InvalidColorSpace {
            description: format!("unsupported color space name: /{unknown}"),
        }),
    }
}

/// Parses a color space defined as an array.
///
/// Array-based color spaces have the form `[/Type param1 param2 ...]`.
fn parse_color_space_array(
    objects: &ObjectCollection,
    arr: &[ObjectVariant],
    depth: usize,
) -> Result<ColorSpace, ColorSpaceError> {
    // Get the color space type (first element)
    let cs_type = arr
        .first()
        .ok_or_else(|| ColorSpaceError::InvalidColorSpace {
            description: "empty color space array".into(),
        })?;

    let cs_type_name = objects.resolve_object(cs_type)?.try_str()?;

    match cs_type_name.as_ref() {
        "Indexed" => parse_indexed_color_space(objects, arr, depth),
        "ICCBased" => parse_icc_based_color_space(objects, arr),
        "Separation" => parse_separation_color_space(objects, arr, depth),
        unknown => Err(ColorSpaceError::InvalidColorSpace {
            description: format!(
                "unsupported color space type: /{unknown} (array with {} elements)",
                arr.len()
            ),
        }),
    }
}

/// Parses a Separation color space: `[/Separation name alternateSpace tintTransform]`
fn parse_separation_color_space(
    objects: &ObjectCollection,
    arr: &[ObjectVariant],
    depth: usize,
) -> Result<ColorSpace, ColorSpaceError> {
    // Expected format: [/Separation name alternateSpace tintTransform]
    let [_, name, alternate_space, tint_transform] = arr else {
        return Err(ColorSpaceError::InvalidColorSpace {
            description: format!("/Separation requires 4 elements, found {}", arr.len()),
        });
    };

    let name = objects.resolve_object(name)?.try_str()?.to_string();
    let alternate_space = parse_color_space_object(objects, alternate_space, depth)?;
    let tint_transform = Function::parse(objects.resolve_object(tint_transform)?, objects)?;

    Ok(ColorSpace::Separation {
        name,
        alternate_space: Box::new(alternate_space),
        tint_transform,
    })
}

/// Parses an Indexed color space: `[/Indexed base hival lookup]`
///
/// - `base`: The base color space for palette entries
/// - `hival`: Maximum index value (0-255)
/// - `lookup`: Lookup table (string or stream)
fn parse_indexed_color_space(
    objects: &ObjectCollection,
    arr: &[ObjectVariant],
    depth: usize,
) -> Result<ColorSpace, ColorSpaceError> {
    // Expected format: [/Indexed base hival lookup]
    let [_, base, hival, lookup] = arr else {
        return Err(ColorSpaceError::InvalidColorSpace {
            description: format!("/Indexed requires 4 elements, found {}", arr.len()),
        });
    };

    let base_cs = parse_color_space_object(objects, base, depth)?;
    let hival = objects.resolve_object(hival)?.as_number::<u8>()?;
    let lookup = extract_lookup_table(objects, lookup)?;

    Ok(ColorSpace::Indexed {
        base: Box::new(base_cs),
        hival,
        lookup,
    })
}

/// Parses an ICCBased color space: `[/ICCBased stream]`
///
/// The stream dictionary must contain an `/N` entry specifying the number of components.
fn parse_icc_based_color_space(
    objects: &ObjectCollection,
    arr: &[ObjectVariant],
) -> Result<ColorSpace, ColorSpaceError> {
    // Expected format: [/ICCBased icc-stream]
    let [_, icc_stream_ref] = arr else {
        return Err(ColorSpaceError::InvalidColorSpace {
            description: format!("/ICCBased requires 2 elements, found {}", arr.len()),
        });
    };

    let icc_stream = objects.resolve_stream(icc_stream_ref)?;
    let num_components = icc_stream
        .dictionary
        .get_or_err("N")?
        .as_number::<usize>()?;

    Ok(ColorSpace::ICCBased { num_components })
}

/// Extracts the lookup table bytes from an Indexed color space.
///
/// The lookup table can be either a string/hex-string or a stream.
fn extract_lookup_table(
    objects: &ObjectCollection,
    lookup: &ObjectVariant,
) -> Result<Vec<u8>, ColorSpaceError> {
    if let Ok(data) = lookup.try_bytes() {
        return Ok(data.to_vec());
    }
    let resolved = objects.resolve_stream(lookup)?;

    let data = resolved.data()?;
    Ok(data.into_owned())
}
