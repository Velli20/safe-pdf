use pdf_graphics::color::Color;
use pdf_object_reader::{FromPdfObject, ObjectAccess, ObjectContext, ReadResult};

use crate::cal_gray_color_space::xyz_to_srgb;
use crate::{cie_color_space::CieColorSpaceParams, error::ColorSpaceError};

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

/// Reads the dictionary operand of a CalRGB color space, `[/CalRGB dict]`.
impl FromPdfObject for CalRGBColorSpace {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut dictionary = context.dictionary()?;
        let CieColorSpaceParams {
            white_point,
            black_point,
        } = CieColorSpaceParams::read(&mut dictionary)?;
        let gamma = dictionary.optional(b"Gamma")?.unwrap_or([1.0, 1.0, 1.0]);
        // Column-major 3×3: [Xa Ya Za Xb Yb Zb Xc Yc Zc], default identity.
        let matrix = dictionary
            .optional(b"Matrix")?
            .unwrap_or([1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]);
        Ok(Self {
            white_point,
            black_point,
            gamma,
            matrix,
        })
    }
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
