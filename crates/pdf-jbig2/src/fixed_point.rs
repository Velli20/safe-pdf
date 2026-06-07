//! Fixed-point arithmetic helpers for JBIG2 numeric fields.

use crate::error::Jbig2Error;

/// Signed JBIG2 fixed-point value with 8 fractional bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Jbig2Fixed8 {
    raw: i64,
}

impl Jbig2Fixed8 {
    const FRACTION_BITS: u32 = 8;

    /// Creates a fixed-point value from a raw signed 8-fraction-bit field.
    pub(crate) fn from_raw_i32(raw: i32) -> Self {
        Self {
            raw: i64::from(raw),
        }
    }

    /// Creates a fixed-point value from a raw unsigned 8-fraction-bit field.
    pub(crate) fn from_raw_u16(raw: u16) -> Self {
        Self {
            raw: i64::from(raw),
        }
    }

    /// Adds two fixed-point values, returning a typed overflow error.
    pub(crate) fn checked_add(
        self,
        rhs: Self,
        overflow_context: &'static str,
    ) -> Result<Self, Jbig2Error> {
        self.raw
            .checked_add(rhs.raw)
            .map(|raw| Self { raw })
            .ok_or(Jbig2Error::Overflow(overflow_context))
    }

    /// Subtracts two fixed-point values, returning a typed overflow error.
    pub(crate) fn checked_sub(
        self,
        rhs: Self,
        overflow_context: &'static str,
    ) -> Result<Self, Jbig2Error> {
        self.raw
            .checked_sub(rhs.raw)
            .map(|raw| Self { raw })
            .ok_or(Jbig2Error::Overflow(overflow_context))
    }

    /// Multiplies a fixed-point value by an unsigned 16-bit grid index.
    pub(crate) fn checked_mul(
        self,
        rhs: u16,
        overflow_context: &'static str,
    ) -> Result<Self, Jbig2Error> {
        self.raw
            .checked_mul(i64::from(rhs))
            .map(|raw| Self { raw })
            .ok_or(Jbig2Error::Overflow(overflow_context))
    }

    /// Converts the fixed-point value to pixels by flooring toward negative infinity.
    pub(crate) fn to_i32_floor(self, overflow_context: &'static str) -> Result<i32, Jbig2Error> {
        let shifted = self
            .raw
            .checked_shr(Self::FRACTION_BITS)
            .ok_or(Jbig2Error::Overflow(overflow_context))?;
        i32::try_from(shifted).map_err(|_| Jbig2Error::Overflow(overflow_context))
    }
}

#[cfg(test)]
mod tests {
    use super::Jbig2Fixed8;
    use crate::error::Jbig2Error;

    const OVERFLOW_CONTEXT: &str = "fixed point test overflow";

    #[test]
    fn converts_raw_fixed_point_value_to_pixel_coordinate() -> Result<(), Jbig2Error> {
        assert_eq!(
            Jbig2Fixed8::from_raw_i32(256).to_i32_floor(OVERFLOW_CONTEXT)?,
            1
        );

        Ok(())
    }

    #[test]
    fn floors_negative_fixed_point_value_to_pixel_coordinate() -> Result<(), Jbig2Error> {
        assert_eq!(
            Jbig2Fixed8::from_raw_i32(-1).to_i32_floor(OVERFLOW_CONTEXT)?,
            -1
        );
        assert_eq!(
            Jbig2Fixed8::from_raw_i32(-256).to_i32_floor(OVERFLOW_CONTEXT)?,
            -1
        );
        assert_eq!(
            Jbig2Fixed8::from_raw_i32(-257).to_i32_floor(OVERFLOW_CONTEXT)?,
            -2
        );

        Ok(())
    }

    #[test]
    fn checks_add_overflow() {
        let err = Jbig2Fixed8 { raw: i64::MAX }
            .checked_add(Jbig2Fixed8::from_raw_u16(1), OVERFLOW_CONTEXT)
            .expect_err("overflow");
        assert_eq!(err, Jbig2Error::Overflow(OVERFLOW_CONTEXT));
    }

    #[test]
    fn checks_subtract_overflow() {
        let err = Jbig2Fixed8 { raw: i64::MIN }
            .checked_sub(Jbig2Fixed8::from_raw_u16(1), OVERFLOW_CONTEXT)
            .expect_err("overflow");
        assert_eq!(err, Jbig2Error::Overflow(OVERFLOW_CONTEXT));
    }

    #[test]
    fn checks_multiply_overflow() {
        let err = Jbig2Fixed8 { raw: i64::MAX }
            .checked_mul(2, OVERFLOW_CONTEXT)
            .expect_err("overflow");
        assert_eq!(err, Jbig2Error::Overflow(OVERFLOW_CONTEXT));
    }

    #[test]
    fn multiplies_fixed_point_value_by_grid_index() -> Result<(), Jbig2Error> {
        let value = Jbig2Fixed8::from_raw_u16(256).checked_mul(3, OVERFLOW_CONTEXT)?;

        assert_eq!(value.to_i32_floor(OVERFLOW_CONTEXT)?, 3);

        Ok(())
    }
}
