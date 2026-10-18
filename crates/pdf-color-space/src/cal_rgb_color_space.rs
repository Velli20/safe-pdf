use pdf_graphics::color::Color;
use pdf_object_reader::{FromPdfObject, ObjectAccess, ObjectContext, ReadResult};

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
        Ok(self.xyz_to_srgb([x, y, z]))
    }

    /// Converts CIE XYZ to gamma-encoded sRGB relative to this space's white point.
    ///
    /// The RGB→XYZ matrix is built from the sRGB primaries' chromaticities, with each
    /// primary scaled so that RGB (1, 1, 1) maps to the white point. Its inverse is the
    /// inverse chromaticity matrix followed by dividing each channel by the white point's
    /// value in that basis.
    fn xyz_to_srgb(&self, xyz: [f32; 3]) -> Color {
        let to_primaries = |[x, y, z]: [f32; 3]| {
            [
                2.088_353_4 * x - 0.990_629_2 * y - 0.321_285_1 * z,
                -1.155_287_8 * x + 2.236_055_3 * y + 0.049_531_46 * z,
                0.066_934_4 * x - 0.245_426_1 * y + 1.271_753_7 * z,
            ]
        };
        let [r, g, b] = to_primaries(xyz);
        let [rw, gw, bw] = to_primaries(self.white_point);
        let scale = |c: f32, w: f32| if w == 0.0 { 0.0 } else { c / w };

        // Linear → gamma-encoded sRGB, clamped to [0, 1]
        let gamma_encode = |c: f32| -> f32 {
            let c = c.clamp(0.0, 1.0);
            if c <= 0.003_130_8 {
                12.92 * c
            } else {
                1.055 * c.powf(1.0 / 2.4) - 0.055
            }
        };

        Color::from_rgb(
            gamma_encode(scale(r, rw)),
            gamma_encode(scale(g, gw)),
            gamma_encode(scale(b, bw)),
        )
    }
}
