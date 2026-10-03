//! Quantization parameters of one subband, and the inverse quantizer.
//!
//! Annex E gives each subband an exponent and mantissa, either listed per
//! subband, derived from the residual subband's pair, or replaced by a plain
//! exponent when the coding is reversible. Those two numbers fix both the
//! step size the inverse quantizer multiplies by and the number of magnitude
//! bit-planes `Mb` the entropy coder may produce, which Tier-1 needs before it
//! can decode anything.

use crate::{
    Jpeg2000Error,
    coefficient::{Coefficient, signed},
    quantization::{Quantization, QuantizationStyle},
    resolution::Subband,
};

/// Bytes in one two-byte exponent and mantissa pair.
const STEP_BYTES: usize = 2;
/// Bits of the mantissa in a two-byte step size.
const MANTISSA_BITS: u32 = 11;
/// Mask selecting the mantissa of a two-byte step size.
const MANTISSA_MASK: u32 = (1 << MANTISSA_BITS) - 1;
/// Shift selecting the exponent from a reversible one-byte entry.
const REVERSIBLE_EXPONENT_SHIFT: u32 = 3;
/// Diagnostic for a quantization table that omits a requested subband.
const MISSING_STEP_REASON: &str = "quantization table does not describe every subband";

/// The quantization parameters in force for one subband.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BandQuantization {
    /// Magnitude bit-planes the entropy coder may produce, `Mb` of Annex E.
    pub(crate) magnitude_bits: u32,
    /// Whether the subband is coded without quantization.
    pub(crate) reversible: bool,
    /// Step-size exponent of the subband.
    pub(crate) exponent: u32,
    /// Step-size mantissa of the subband.
    pub(crate) mantissa: u32,
}

impl BandQuantization {
    /// Reads the parameters of one subband from a QCD or QCC segment.
    ///
    /// # Errors
    ///
    /// Returns `InvalidMarker` if the segment's step-size table does not
    /// describe the subband.
    pub(crate) fn new(
        quantization: &Quantization<'_>,
        band: &Subband,
    ) -> Result<Self, Jpeg2000Error> {
        let (exponent, mantissa) = match quantization.style {
            QuantizationStyle::None => (Self::reversible_exponent(quantization, band)?, 0),
            QuantizationStyle::Derived => Self::derived(quantization, band)?,
            QuantizationStyle::Expounded => Self::expounded(quantization, band)?,
        };
        Ok(Self {
            magnitude_bits: u32::from(quantization.guard_bits)
                .saturating_add(exponent)
                .saturating_sub(1),
            reversible: quantization.style == QuantizationStyle::None,
            exponent,
            mantissa,
        })
    }

    /// Reads the one-byte exponent a reversible codestream lists per subband.
    fn reversible_exponent(
        quantization: &Quantization<'_>,
        band: &Subband,
    ) -> Result<u32, Jpeg2000Error> {
        let byte = quantization
            .steps
            .get(band.step_index)
            .ok_or_else(|| quantization.site().invalid(MISSING_STEP_REASON))?;
        Ok(u32::from(*byte >> REVERSIBLE_EXPONENT_SHIFT))
    }

    /// Derives a subband's pair from the residual subband's pair.
    ///
    /// Annex E.1.1 lowers the exponent by one for each resolution level above
    /// the residual, leaving the mantissa unchanged.
    fn derived(
        quantization: &Quantization<'_>,
        band: &Subband,
    ) -> Result<(u32, u32), Jpeg2000Error> {
        let (exponent, mantissa) = Self::pair(quantization, 0)?;
        let levels_above = band.step_index.saturating_sub(1) / 3;
        let levels_above = u32::try_from(levels_above)
            .map_err(|_| quantization.site().invalid(MISSING_STEP_REASON))?;
        Ok((exponent.saturating_sub(levels_above), mantissa))
    }

    /// Reads the pair a codestream lists explicitly for this subband.
    fn expounded(
        quantization: &Quantization<'_>,
        band: &Subband,
    ) -> Result<(u32, u32), Jpeg2000Error> {
        Self::pair(quantization, band.step_index)
    }

    /// Reads the exponent and mantissa of one two-byte table entry.
    fn pair(quantization: &Quantization<'_>, index: usize) -> Result<(u32, u32), Jpeg2000Error> {
        let start = index
            .checked_mul(STEP_BYTES)
            .ok_or_else(|| quantization.site().invalid(MISSING_STEP_REASON))?;
        let end = start
            .checked_add(STEP_BYTES)
            .ok_or_else(|| quantization.site().invalid(MISSING_STEP_REASON))?;
        let [high, low] = quantization
            .steps
            .get(start..end)
            .and_then(|bytes| <[u8; STEP_BYTES]>::try_from(bytes).ok())
            .ok_or_else(|| quantization.site().invalid(MISSING_STEP_REASON))?;
        let value = u32::from(u16::from_be_bytes([high, low]));
        Ok((value >> MANTISSA_BITS, value & MANTISSA_MASK))
    }
}

/// Turns decoded code-block magnitudes into subband coefficients.
///
/// Annex E.1.1 reconstructs a coefficient as `(q + r * 2^Nb) * step`, where
/// `q` is the decoded magnitude, `Nb` counts the bit-planes that were never
/// decoded, and `r` is one half. The offset stands for the middle of the
/// interval the magnitude represents, so a quantized subband keeps it even
/// when every bit-plane arrived: `2^0 / 2` is still half a step. A reversible
/// subband is not quantized, so a fully decoded magnitude is already exact
/// and only undecoded planes call for an offset.
///
/// A region of interest coded with the Maxshift method of Annex H.2 is undone
/// between the two, because the encoder applied it after quantization.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Reconstruction {
    quantization: BandQuantization,
    undecoded_planes: u32,
    roi_shift: u32,
    step: f64,
}

impl Reconstruction {
    /// Prepares the reconstruction of one code-block's coefficients.
    ///
    /// `precision` is the component's declared sample precision and `gain`
    /// the subband's log-two gain, which together give the dynamic range
    /// `Rb` the step size is relative to.
    pub(crate) fn new(
        quantization: BandQuantization,
        undecoded_planes: u32,
        roi_shift: u8,
        precision: u8,
        gain: u8,
    ) -> Self {
        let range = u32::from(precision).saturating_add(u32::from(gain));
        let exponent = i32::try_from(range)
            .unwrap_or(i32::MAX)
            .saturating_sub(i32::try_from(quantization.exponent).unwrap_or(i32::MAX));
        let mantissa = 1.0 + mantissa_fraction(quantization.mantissa);
        Self {
            quantization,
            undecoded_planes,
            roi_shift: u32::from(roi_shift),
            step: mantissa * power_of_two(exponent),
        }
    }

    /// Returns the magnitude with its whole-numbered offset and region shift.
    ///
    /// Only the whole part of `r * 2^Nb` fits in an integer magnitude; the
    /// remaining half step is added by [`Reconstruction::real`], which works
    /// in real numbers.
    fn magnitude(self, magnitude: u64) -> u64 {
        if magnitude == 0 {
            return 0;
        }
        let half = self
            .undecoded_planes
            .checked_sub(1)
            .and_then(|plane| 1u64.checked_shl(plane))
            .unwrap_or(0);
        let magnitude = magnitude.saturating_add(half);
        let threshold = 1u64.checked_shl(self.roi_shift).unwrap_or(u64::MAX);
        if self.roi_shift > 0 && magnitude >= threshold {
            return magnitude.checked_shr(self.roi_shift).unwrap_or(0);
        }
        magnitude
    }

    /// Returns the fraction of a step the whole magnitude could not carry.
    ///
    /// With every bit-plane decoded the offset is exactly half a step, and it
    /// is zero only when the magnitude itself is zero.
    fn fractional_offset(self, magnitude: u64) -> f64 {
        if magnitude == 0 || self.undecoded_planes > 0 {
            return 0.0;
        }
        0.5
    }

    /// Returns one reversible coefficient, which needs no step size.
    ///
    /// # Errors
    ///
    /// Returns `Overflow` when the magnitude does not fit the chosen width.
    pub(crate) fn integer<C: Coefficient>(
        self,
        magnitude: u64,
        negative: bool,
    ) -> Result<C, Jpeg2000Error> {
        signed(self.magnitude(magnitude), negative)
    }

    /// Returns one irreversible coefficient, scaled by the step size.
    pub(crate) fn real(self, magnitude: u64, negative: bool) -> f64 {
        let offset = self.fractional_offset(magnitude);
        let scaled = (whole_to_real(self.magnitude(magnitude)) + offset) * self.step;
        if negative { -scaled } else { scaled }
    }
}

/// Returns the fractional part a step-size mantissa contributes.
fn mantissa_fraction(mantissa: u32) -> f64 {
    whole_to_real(u64::from(mantissa)) / whole_to_real(1 << MANTISSA_BITS)
}

/// Returns `2^exponent` as a real number, saturating at the extremes.
fn power_of_two(exponent: i32) -> f64 {
    2.0_f64.powi(exponent)
}

/// Converts an unsigned magnitude to the nearest representable real number.
fn whole_to_real(value: u64) -> f64 {
    #[allow(clippy::as_conversions)]
    {
        value as f64
    }
}
