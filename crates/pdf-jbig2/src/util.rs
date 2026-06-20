//! Shared JBIG2 decoding helpers.

use crate::error::Jbig2Error;

pub(crate) const INTEGER_CONVERSION_OVERFLOW: &str = "integer conversion overflow";
pub(crate) const IMAGE_DIMENSIONS_OVERFLOW: &str = "image dimensions overflow";

pub(crate) fn packed_row_len(width: u16) -> Result<usize, Jbig2Error> {
    usize::from(width)
        .checked_add(7)
        .map(|value| value / 8)
        .ok_or(Jbig2Error::Overflow(IMAGE_DIMENSIONS_OVERFLOW))
}

pub(crate) fn ceil_log2(value: usize) -> Result<u8, Jbig2Error> {
    if value <= 1 {
        return Ok(0);
    }
    let bits_u32 = usize::BITS.saturating_sub((value.saturating_sub(1)).leading_zeros());
    u8::try_from(bits_u32).map_err(|_| Jbig2Error::Overflow(INTEGER_CONVERSION_OVERFLOW))
}

pub(crate) fn i32_to_usize(value: i32) -> Result<usize, Jbig2Error> {
    usize::try_from(value).map_err(|_| Jbig2Error::Overflow(INTEGER_CONVERSION_OVERFLOW))
}

pub(crate) fn i32_to_u16(value: i32, label: &'static str) -> Result<u16, Jbig2Error> {
    u16::try_from(value).map_err(|_| Jbig2Error::InvalidState(label))
}

pub(crate) fn usize_to_u16(value: usize, label: &'static str) -> Result<u16, Jbig2Error> {
    u16::try_from(value).map_err(|_| Jbig2Error::InvalidState(label))
}

pub(crate) fn refinement_reference_offset(size_delta: i32, delta: i32) -> Result<i32, Jbig2Error> {
    (size_delta >> 1)
        .checked_add(delta)
        .ok_or(Jbig2Error::Overflow(INTEGER_CONVERSION_OVERFLOW))
}

pub(crate) fn refined_dimension(
    base: u16,
    delta: i32,
    label: &'static str,
) -> Result<u16, Jbig2Error> {
    let value = i32::from(base)
        .checked_add(delta)
        .ok_or(Jbig2Error::Overflow(INTEGER_CONVERSION_OVERFLOW))?;
    u16::try_from(value).map_err(|_| Jbig2Error::InvalidState(label))
}

#[cfg(test)]
mod tests {}
