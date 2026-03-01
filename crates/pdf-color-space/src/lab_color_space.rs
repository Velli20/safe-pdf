use pdf_graphics::color::Color;
use pdf_object::{object_resolver::ObjectResolver, object_variant::ObjectVariant};

use crate::color_space::{ColorSpace, ColorSpaceError};

/// CIE 1976 L*a*b* color space.
///
/// A device-independent, perceptually uniform color space.
/// Components: L* (0–100), a* (green–red), b* (blue–yellow).
#[derive(Debug, Clone)]
pub struct LabColorSpace {
    /// Reference white in XYZ [Xw, Yw, Zw]. Required.
    pub white_point: [f32; 3],
    /// Reference black in XYZ [Xb, Yb, Zb]. Default [0, 0, 0].
    pub black_point: [f32; 3],
    /// Valid range for a* and b*: [amin, amax, bmin, bmax].
    /// Default [-100, 100, -100, 100].
    pub range: [f32; 4],
}

/// Parses a Lab color space: `[/Lab dict]`
///
/// The dictionary must contain a `WhitePoint` entry and may contain `BlackPoint`
/// and `Range` entries.
pub(crate) fn parse_lab_color_space(
    objects: &dyn ObjectResolver,
    arr: &[ObjectVariant],
) -> Result<ColorSpace, ColorSpaceError> {
    let [_, dict_obj] = arr else {
        return Err(ColorSpaceError::InvalidColorSpace {
            description: format!("/Lab requires 2 elements, found {}", arr.len()),
        });
    };
    let dict = dict_obj.try_dictionary(objects)?;
    let white_point = dict
        .get_or_err("WhitePoint")?
        .try_array_of::<f32, 3>(objects)?;
    let black_point = dict
        .get("BlackPoint")
        .map(|bp| bp.try_array_of::<f32, 3>(objects))
        .transpose()?
        .unwrap_or([0.0, 0.0, 0.0]);
    let range = dict
        .get("Range")
        .map(|r| r.try_array_of::<f32, 4>(objects))
        .transpose()?
        .unwrap_or([-100.0, 100.0, -100.0, 100.0]);
    Ok(ColorSpace::Lab(LabColorSpace {
        white_point,
        black_point,
        range,
    }))
}

impl LabColorSpace {
    pub(crate) fn apply(&self, components: &[f32]) -> Result<Color, ColorSpaceError> {
        let [l, a, b] = components else {
            return Err(ColorSpaceError::InsufficientComponents(3, components.len()));
        };
        let [amin, amax, bmin, bmax] = self.range;
        Ok(Color::from_lab(
            // According to PDF specification, L* shall be in [0.0, 100.0].
            (*l).clamp(0.0, 100.0),
            (*a).clamp(amin, amax),
            (*b).clamp(bmin, bmax),
            self.white_point,
        ))
    }
}
