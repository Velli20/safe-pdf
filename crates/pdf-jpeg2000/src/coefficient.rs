//! The integer widths reconstructed samples can take.
//!
//! Part 1 allows component precisions up to 38 bits, and guard bits push a
//! code-block's magnitude above that again, so the widest images need 64-bit
//! coefficients. Almost none do, and carrying 64 bits through a whole tile
//! would double the working set of every ordinary image, so the reconstruction
//! pipeline is written once against this trait and instantiated at the
//! narrowest width that can hold the tile's coefficients.

use crate::{Jpeg2000Error, tile::SampleData};

/// An integer width the reconstruction pipeline can be instantiated at.
pub(crate) trait Coefficient: Copy + Default + core::fmt::Debug + Ord + Send + Sync {
    /// Bits available for a signed value of this width.
    const BITS: u32;

    /// Converts a value from the widest representation, if it fits.
    fn from_i64(value: i64) -> Option<Self>;

    /// Converts a value to the widest representation.
    fn to_i64(self) -> i64;

    /// Rounds a real reconstruction to this width, saturating on overflow.
    ///
    /// Ties round to the nearest even integer, which is the rule Part 1's
    /// reference conversions use and keeps the error unbiased.
    fn from_real(value: f64) -> Self;

    /// Shifts right arithmetically, which floors towards negative infinity.
    fn shift_right(self, amount: u32) -> Self;

    /// Borrows a run of samples in the variant a tile sink expects.
    fn sample_data(samples: &[Self]) -> SampleData<'_>;
}

impl Coefficient for i32 {
    const BITS: u32 = i32::BITS;

    fn from_i64(value: i64) -> Option<Self> {
        Self::try_from(value).ok()
    }

    fn to_i64(self) -> i64 {
        i64::from(self)
    }

    fn from_real(value: f64) -> Self {
        round_to_i64(value)
            .try_into()
            .unwrap_or(if value < 0.0 { Self::MIN } else { Self::MAX })
    }

    fn shift_right(self, amount: u32) -> Self {
        self.checked_shr(amount).unwrap_or(self >> (Self::BITS - 1))
    }

    fn sample_data(samples: &[Self]) -> SampleData<'_> {
        SampleData::I32(samples)
    }
}

impl Coefficient for i64 {
    const BITS: u32 = i64::BITS;

    fn from_i64(value: i64) -> Option<Self> {
        Some(value)
    }

    fn to_i64(self) -> i64 {
        self
    }

    fn from_real(value: f64) -> Self {
        round_to_i64(value)
    }

    fn shift_right(self, amount: u32) -> Self {
        self.checked_shr(amount).unwrap_or(self >> (Self::BITS - 1))
    }

    fn sample_data(samples: &[Self]) -> SampleData<'_> {
        SampleData::I64(samples)
    }
}

/// Rounds a real reconstruction to the nearest integer, saturating.
///
/// Ties round to even, matching the rounding the reference conversions of
/// Part 1 use.
fn round_to_i64(value: f64) -> i64 {
    if value.is_nan() {
        return 0;
    }
    let rounded = value.round_ties_even();
    if rounded >= as_real(i64::MAX) {
        return i64::MAX;
    }
    if rounded <= as_real(i64::MIN) {
        return i64::MIN;
    }
    #[allow(clippy::as_conversions)]
    {
        rounded as i64
    }
}

/// Converts an integer to the nearest representable real number.
fn as_real(value: i64) -> f64 {
    #[allow(clippy::as_conversions)]
    {
        value as f64
    }
}

/// Chooses the coefficient width a tile's magnitudes need.
///
/// The widest code-block magnitude a tile can produce is its largest `Mb`
/// plus a sign, so anything that fits in a signed 32-bit value is decoded at
/// that width.
pub(crate) fn fits_in_32_bits(magnitude_bits: u32) -> bool {
    magnitude_bits < i32::BITS.saturating_sub(1)
}

/// Returns a coefficient value from a decoded magnitude and sign.
///
/// # Errors
///
/// Returns `Overflow` when the magnitude does not fit the chosen width.
pub(crate) fn signed<C: Coefficient>(magnitude: u64, negative: bool) -> Result<C, Jpeg2000Error> {
    let magnitude = i64::try_from(magnitude).map_err(|_| Jpeg2000Error::Overflow {
        context: "coefficient magnitude",
    })?;
    let value = if negative {
        magnitude.checked_neg()
    } else {
        Some(magnitude)
    };
    value.and_then(C::from_i64).ok_or(Jpeg2000Error::Overflow {
        context: "coefficient magnitude",
    })
}
