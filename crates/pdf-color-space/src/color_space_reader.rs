use pdf_object::{
    dictionary::Dictionary, object_resolver::ObjectResolver, object_variant::ObjectVariant,
};

use crate::{
    cal_gray_color_space::parse_cal_gray_color_space,
    cal_rgb_color_space::parse_cal_rgb_color_space,
    device_n_color_space::parse_device_n_color_space,
    icc_based_color_space::parse_icc_based_color_space,
    indexed_color_space::parse_indexed_color_space, lab_color_space::parse_lab_color_space,
    separation_color_space::parse_separation_color_space,
};
use crate::{color_space::ColorSpace, error::ColorSpaceError};

/// Maximum nesting depth for color space definitions.
///
/// Prevents stack overflow from maliciously crafted PDFs with deeply nested
/// color spaces (e.g., Indexed within Indexed within Indexed...).
const MAX_COLOR_SPACE_DEPTH: usize = 8;

impl ColorSpace {
    const KEY: &'static str = "ColorSpace";

    pub fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Option<ColorSpace>, ColorSpaceError> {
        let Some(color_space_obj) = dictionary.get(Self::KEY) else {
            return Ok(None);
        };

        parse_color_space_object(objects, color_space_obj, 0).map(Some)
    }

    /// Parses a color space directly from an [`ObjectVariant`].
    ///
    /// Accepts names, arrays, and indirect references — all valid forms of a
    /// color space definition as they appear in a resource dictionary value.
    pub fn from_object(
        obj: &ObjectVariant,
        objects: &dyn ObjectResolver,
    ) -> Result<ColorSpace, ColorSpaceError> {
        parse_color_space_object(objects, obj, 0)
    }
}

/// Parses a color space from a PDF object.
///
/// Color spaces can be specified as:
/// - A name (e.g., `/DeviceRGB`)
/// - An array (e.g., `[/Indexed /DeviceRGB 255 <lookup data>]`)
pub(crate) fn parse_color_space_object(
    objects: &dyn ObjectResolver,
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
        other => ColorSpace::try_from(other.try_str(objects)?),
    }
}

/// Parses a color space defined as an array.
///
/// Array-based color spaces have the form `[/Type param1 param2 ...]`.
fn parse_color_space_array(
    objects: &dyn ObjectResolver,
    arr: &[ObjectVariant],
    depth: usize,
) -> Result<ColorSpace, ColorSpaceError> {
    if let [single] = arr {
        return ColorSpace::try_from(single.try_str(objects)?);
    }

    // Get the color space type (first element)
    let cs_type = arr
        .first()
        .ok_or_else(|| ColorSpaceError::InvalidColorSpace {
            description: "empty color space array".into(),
        })?;

    match cs_type.try_str(objects)? {
        "Indexed" => parse_indexed_color_space(objects, arr, depth),
        "ICCBased" => parse_icc_based_color_space(objects, arr, depth),
        "Separation" => parse_separation_color_space(objects, arr, depth),
        "Lab" => parse_lab_color_space(objects, arr),
        "CalGray" => parse_cal_gray_color_space(objects, arr),
        "CalRGB" => parse_cal_rgb_color_space(objects, arr),
        "DeviceN" => parse_device_n_color_space(objects, arr, depth),
        "Pattern" => {
            // Optional second element is the underlying color space used with
            // uncolored tiling patterns.
            let underlying = arr
                .get(1)
                .map(|obj| parse_color_space_object(objects, obj, depth).map(Box::new))
                .transpose()?;
            Ok(ColorSpace::Pattern(underlying))
        }
        unknown => Err(ColorSpaceError::InvalidColorSpace {
            description: format!(
                "unsupported color space type: /{unknown} (array with {} elements)",
                arr.len()
            ),
        }),
    }
}

#[cfg(test)]
mod tests {
    use pdf_object::{object_resolver::PassthroughResolver, object_variant::ObjectVariant};

    use crate::{color_space::ColorSpace, error::ColorSpaceError};

    fn name(value: &str) -> ObjectVariant {
        ObjectVariant::Name(value.as_bytes().to_vec())
    }

    #[test]
    fn parses_single_name_device_color_space_arrays() {
        let cases = [
            (
                ObjectVariant::Array(vec![name("DeviceGray")]),
                ColorSpace::DeviceGray,
            ),
            (
                ObjectVariant::Array(vec![name("DeviceRGB")]),
                ColorSpace::DeviceRGB,
            ),
            (
                ObjectVariant::Array(vec![name("DeviceCMYK")]),
                ColorSpace::DeviceCMYK,
            ),
        ];

        for (object, expected) in cases {
            let parsed = ColorSpace::from_object(&object, &PassthroughResolver).unwrap();

            match (parsed, expected) {
                (ColorSpace::DeviceGray, ColorSpace::DeviceGray)
                | (ColorSpace::DeviceRGB, ColorSpace::DeviceRGB)
                | (ColorSpace::DeviceCMYK, ColorSpace::DeviceCMYK) => {}
                (parsed, expected) => {
                    panic!("expected {expected:?} for single-name array, got {parsed:?}");
                }
            }
        }
    }

    #[test]
    fn parses_single_name_pattern_array() {
        let parsed = ColorSpace::from_object(
            &ObjectVariant::Array(vec![name("Pattern")]),
            &PassthroughResolver,
        )
        .unwrap();

        assert!(matches!(parsed, ColorSpace::Pattern(None)));
    }

    #[test]
    fn rejects_empty_color_space_array() {
        let error =
            ColorSpace::from_object(&ObjectVariant::Array(Vec::new()), &PassthroughResolver)
                .unwrap_err();

        assert!(matches!(
            error,
            ColorSpaceError::InvalidColorSpace { description } if description == "empty color space array"
        ));
    }

    #[test]
    fn rejects_unsupported_multi_element_color_space_array() {
        let error = ColorSpace::from_object(
            &ObjectVariant::Array(vec![name("DeviceGray"), ObjectVariant::Integer(1)]),
            &PassthroughResolver,
        )
        .unwrap_err();

        assert!(matches!(
            error,
            ColorSpaceError::InvalidColorSpace { description }
                if description == "unsupported color space type: /DeviceGray (array with 2 elements)"
        ));
    }
}
