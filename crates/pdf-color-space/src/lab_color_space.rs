use pdf_graphics::color::Color;
use pdf_object_reader::{FromPdfObject, ObjectAccess, ObjectContext, ReadResult};

use crate::{cie_color_space::CieColorSpaceParams, error::ColorSpaceError};

/// Reference white used to convert L*a*b* values to XYZ.
///
/// Conversion uses this fixed daylight white rather than the space's `WhitePoint`, so a
/// document white point never tints the result.
const LAB_REFERENCE_WHITE: [f32; 3] = [0.957, 1.0, 1.0889];

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
        let object = context.object();
        let mut dictionary = context.dictionary()?;
        let CieColorSpaceParams {
            white_point,
            black_point,
        } = dictionary.read(object)?;
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
            LAB_REFERENCE_WHITE,
        ))
    }

    /// Converts one 8-bit encoded `L*a*b*` triple, such as an Indexed palette entry,
    /// to a [`Color`].
    ///
    /// Each byte spans its component's full range linearly: `0..=255` maps L* onto
    /// `0..=100`, and a* and b* onto the space's `[amin amax]` and `[bmin bmax]` ranges.
    pub(crate) fn apply_encoded(&self, [l, a, b]: [u8; 3]) -> Result<Color, ColorSpaceError> {
        let [amin, amax, bmin, bmax] = self.range;
        let fraction = |byte: u8| f32::from(byte) / 255.0;
        self.apply(&[
            fraction(l) * 100.0,
            amin + fraction(a) * (amax - amin),
            bmin + fraction(b) * (bmax - bmin),
        ])
    }
}
