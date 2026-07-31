use pdf_graphics::color::Color;
use pdf_object::object_lookup::ObjectLookupExt;
use pdf_object::{object_resolver::ObjectResolver, object_variant::ObjectVariant};

use crate::cal_gray_color_space::xyz_to_srgb;
use crate::{
    cie_color_space::CieColorSpaceParams, color_space::ColorSpace, error::ColorSpaceError,
};

/// Calibrated RGB color space.
///
/// A device-independent three-component color space defined in terms of
/// the CIE XYZ model. Components A, B, C are each in [0.0, 1.0].
#[derive(Debug, Clone)]
pub struct CalRGBColorSpace {
    /// Reference white point in XYZ [Xw, Yw, Zw]. Required.
    pub white_point: [f32; 3],
    /// Reference black point in XYZ. Default [0.0, 0.0, 0.0].
    pub black_point: [f32; 3],
    /// Per-component gamma exponents [GA, GB, GC]. Default [1.0, 1.0, 1.0].
    pub gamma: [f32; 3],
    /// Column-major 3×3 linear transformation matrix.
    ///
    /// Layout: [Xa Ya Za Xb Yb Zb Xc Yc Zc], where columns A, B, C map
    /// gamma-corrected components to XYZ. Default is the identity matrix.
    pub matrix: [f32; 9],
}

/// Parses a CalRGB color space: `[/CalRGB dict]`
pub(crate) fn parse_cal_rgb_color_space(
    objects: &dyn ObjectResolver,
    arr: &[ObjectVariant],
) -> Result<ColorSpace, ColorSpaceError> {
    let [_, dict_obj] = arr else {
        return Err(ColorSpaceError::InvalidColorSpace {
            description: format!("/CalRGB requires 2 elements, found {}", arr.len()),
        });
    };
    let dict = dict_obj.try_dictionary(objects)?;
    let CieColorSpaceParams {
        white_point,
        black_point,
    } = CieColorSpaceParams::from_dictionary(dict, objects)?;
    let gamma = dict
        .optional_array_of::<f32, 3>("Gamma", objects)?
        .unwrap_or([1.0, 1.0, 1.0]);
    // Column-major 3×3: [Xa Ya Za Xb Yb Zb Xc Yc Zc], default identity.
    let matrix = dict
        .optional_array_of::<f32, 9>("Matrix", objects)?
        .unwrap_or([1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]);
    Ok(ColorSpace::CalRGB(CalRGBColorSpace {
        white_point,
        black_point,
        gamma,
        matrix,
    }))
}

impl CalRGBColorSpace {
    pub(crate) fn apply(&self, components: &[f32]) -> Result<Color, ColorSpaceError> {
        let [a, b, c] = components else {
            return Err(ColorSpaceError::InsufficientComponents(3, components.len()));
        };
        let [ga, gb, gc] = self.gamma;
        // Apply per-component gamma correction
        let ag = a.clamp(0.0, 1.0).powf(ga);
        let bg = b.clamp(0.0, 1.0).powf(gb);
        let cg = c.clamp(0.0, 1.0).powf(gc);
        // Apply column-major matrix [Xa Ya Za Xb Yb Zb Xc Yc Zc] to get XYZ
        let [xa, ya, za, xb, yb, zb, xc, yc, zc] = self.matrix;
        let x = xa * ag + xb * bg + xc * cg;
        let y = ya * ag + yb * bg + yc * cg;
        let z = za * ag + zb * bg + zc * cg;
        Ok(xyz_to_srgb(x, y, z))
    }
}
