use pdf_graphics::color::Color;
use pdf_object_reader::{FromPdfObject, ObjectAccess, ObjectContext, ReadResult};

use crate::{cie_color_space::CieColorSpaceParams, error::ColorSpaceError};

/// Calibrated Gray color space.
///
/// A device-independent single-component color space defined in terms of the
/// CIE XYZ model. The single component A is in [0.0, 1.0].
#[derive(Debug, Clone)]
pub struct CalGrayColorSpace {
    /// Reference white point in XYZ [Xw, Yw, Zw]. Required.
    pub white_point: [f32; 3],
    /// Reference black point in XYZ [Xb, Yb, Zb]. Default [0.0, 0.0, 0.0].
    pub black_point: [f32; 3],
    /// Exponent applied to the gray component before mapping to XYZ. Default 1.0.
    pub gamma: f32,
}

/// Reads the dictionary operand of a CalGray color space, `[/CalGray dict]`.
impl FromPdfObject for CalGrayColorSpace {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut dictionary = context.dictionary()?;
        let CieColorSpaceParams {
            white_point,
            black_point,
        } = CieColorSpaceParams::read(&mut dictionary)?;
        let gamma = dictionary.optional(b"Gamma")?.unwrap_or(1.0);
        Ok(Self {
            white_point,
            black_point,
            gamma,
        })
    }
}

impl CalGrayColorSpace {
    pub(crate) fn apply(&self, components: &[f32]) -> Result<Color, ColorSpaceError> {
        let [a] = components else {
            return Err(ColorSpaceError::InsufficientComponents(1, components.len()));
        };
        let a = a.clamp(0.0, 1.0);
        // X = Xw * A^Gamma, Y = Yw * A^Gamma, Z = Zw * A^Gamma
        let a_gamma = a.powf(self.gamma);
        let [xw, yw, zw] = self.white_point;
        Ok(xyz_to_srgb(xw * a_gamma, yw * a_gamma, zw * a_gamma))
    }
}

/// Converts CIE XYZ to sRGB using the IEC 61966-2-1 D65 matrix and gamma encoding.
///
/// This is an approximation when the source white point differs from D65;
/// proper colour management would require a chromatic adaptation transform.
pub(crate) fn xyz_to_srgb(x: f32, y: f32, z: f32) -> Color {
    // XYZ → linear sRGB (IEC 61966-2-1 D65 matrix)
    let r_lin = 3.240_454_2 * x - 1.537_138_5 * y - 0.498_531_4 * z;
    let g_lin = -0.969_266 * x + 1.876_010_8 * y + 0.041_556 * z;
    let b_lin = 0.055_643_4 * x - 0.204_025_9 * y + 1.057_225_2 * z;

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
        gamma_encode(r_lin),
        gamma_encode(g_lin),
        gamma_encode(b_lin),
    )
}
