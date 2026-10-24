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
        let object = context.object();
        let mut dictionary = context.dictionary()?;
        let CieColorSpaceParams {
            white_point,
            black_point,
        } = dictionary.read(object)?;
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
        // The gray level is used as is; the white point and gamma do not tint it.
        let a = a.clamp(0.0, 1.0);
        Ok(Color::from_rgb(a, a, a))
    }
}
