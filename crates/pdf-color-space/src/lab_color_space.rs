use pdf_graphics::color::Color;
use pdf_object_reader::{FromPdfObject, ObjectAccess, ObjectContext, ReadResult};

use crate::{cie_color_space::CieColorSpaceParams, error::ColorSpaceError};

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

/// Reads the dictionary operand of a Lab color space, `[/Lab dict]`.
///
/// The dictionary must contain a `WhitePoint` entry and may contain `BlackPoint`
/// and `Range` entries.
impl FromPdfObject for LabColorSpace {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut dictionary = context.dictionary()?;
        let CieColorSpaceParams {
            white_point,
            black_point,
        } = CieColorSpaceParams::read(&mut dictionary)?;
        let range = dictionary
            .optional(b"Range")?
            .unwrap_or([-100.0, 100.0, -100.0, 100.0]);
        Ok(Self {
            white_point,
            black_point,
            range,
        })
    }
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
