//! Bounded, checked cursors over borrowed encoded bytes.
//!
//! Every read is length checked and reports its failure at the offset of the
//! field that could not be read, expressed in the original caller-owned buffer.
//! Cursors borrow their input and never allocate, so a subcursor built from a
//! bounded payload cannot read into the following segment.

use crate::{Jbig2Error, Result, error::SourceLocation};

/// Big-endian byte reader over one bounded slice.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ByteCursor<'a> {
    input: &'a [u8],
    position: usize,
    origin: SourceLocation,
}

impl<'a> ByteCursor<'a> {
    /// Begin reading at the start of `input`, whose first byte lies at `origin`.
    pub(crate) fn new(input: &'a [u8], origin: SourceLocation) -> Self {
        Self {
            input,
            position: 0,
            origin,
        }
    }

    /// Return the number of bytes consumed so far.
    pub(crate) fn position(self) -> usize {
        self.position
    }

    /// Locate the next unread byte in the original buffer.
    ///
    /// Diagnostic offsets saturate: a position beyond `usize::MAX` cannot occur
    /// for a slice that exists, and an error location is never used to index.
    pub(crate) fn location(self) -> SourceLocation {
        self.location_at(self.position)
    }

    /// Borrow the unread suffix for the original input lifetime.
    pub(crate) fn remaining(self) -> &'a [u8] {
        self.input.get(self.position..).unwrap_or(&[])
    }

    /// Borrow the next `count` bytes and advance past them.
    pub(crate) fn take(&mut self, count: usize) -> Result<&'a [u8]> {
        let end = self
            .position
            .checked_add(count)
            .ok_or(Jbig2Error::Overflow("field extent"))?;
        let bytes = self
            .input
            .get(self.position..end)
            .ok_or_else(|| Jbig2Error::Truncated {
                location: self.location(),
                needed: end.saturating_sub(self.input.len()),
            })?;
        self.position = end;
        Ok(bytes)
    }

    /// Inspect the next byte without advancing.
    pub(crate) fn peek_u8(self) -> Result<u8> {
        self.input
            .get(self.position)
            .copied()
            .ok_or_else(|| Jbig2Error::Truncated {
                location: self.location(),
                needed: 1,
            })
    }

    /// Read one byte.
    pub(crate) fn u8(&mut self) -> Result<u8> {
        let location = self.location();
        self.take(1)?
            .first()
            .copied()
            .ok_or(Jbig2Error::InvalidLength(location))
    }

    /// Read a sixteen-bit big-endian field.
    pub(crate) fn u16(&mut self) -> Result<u16> {
        Ok(self
            .take(2)?
            .iter()
            .fold(0_u16, |value, byte| (value << 8) | u16::from(*byte)))
    }

    /// Read a thirty-two-bit big-endian field.
    pub(crate) fn u32(&mut self) -> Result<u32> {
        Ok(self
            .take(4)?
            .iter()
            .fold(0_u32, |value, byte| (value << 8) | u32::from(*byte)))
    }

    /// Read a thirty-two-bit big-endian field as a two's-complement signed value.
    pub(crate) fn i32(&mut self) -> Result<i32> {
        Ok(i32::from_be_bytes(self.u32()?.to_be_bytes()))
    }

    /// Read a big-endian unsigned field of `width` bytes, at most four.
    pub(crate) fn uint(&mut self, width: usize) -> Result<u32> {
        if width > 4 {
            return Err(Jbig2Error::Overflow("field width"));
        }
        Ok(self
            .take(width)?
            .iter()
            .fold(0_u32, |value, byte| (value << 8) | u32::from(*byte)))
    }

    /// Locate a byte at `offset` from the start of this cursor's input.
    pub(crate) fn location_at(self, offset: usize) -> SourceLocation {
        SourceLocation {
            origin: self.origin.origin,
            byte_offset: self.origin.byte_offset.saturating_add(offset),
            bit_offset: None,
        }
    }
}

/// Most-significant-bit-first bit reader over one bounded slice.
#[derive(Debug, Clone, Copy)]
pub(crate) struct BitCursor<'a> {
    input: &'a [u8],
    bit_position: usize,
    origin: SourceLocation,
}

impl<'a> BitCursor<'a> {
    /// Begin reading at the first bit of `input`, which lies at `origin`.
    pub(crate) fn new(input: &'a [u8], origin: SourceLocation) -> Self {
        Self {
            input,
            bit_position: 0,
            origin,
        }
    }

    /// Return the number of bits consumed so far.
    pub(crate) fn bit_position(self) -> usize {
        self.bit_position
    }

    /// Locate the next unread bit in the original buffer.
    pub(crate) fn location(self) -> SourceLocation {
        SourceLocation {
            origin: self.origin.origin,
            byte_offset: self
                .origin
                .byte_offset
                .saturating_add(self.bit_position / 8),
            bit_offset: u8::try_from(self.bit_position % 8).ok(),
        }
    }

    /// Read one bit.
    pub(crate) fn bit(&mut self) -> Result<bool> {
        let byte = *self
            .input
            .get(self.bit_position / 8)
            .ok_or_else(|| Jbig2Error::Truncated {
                location: self.location(),
                needed: 1,
            })?;
        let value = byte & (0x80_u8 >> (self.bit_position % 8)) != 0;
        self.bit_position = self
            .bit_position
            .checked_add(1)
            .ok_or(Jbig2Error::Overflow("bit position"))?;
        Ok(value)
    }

    /// Read a fixed-width big-endian field of at most thirty-two bits.
    pub(crate) fn bits(&mut self, count: u8) -> Result<u32> {
        if count > 32 {
            return Err(Jbig2Error::InvalidCodingData {
                location: self.location(),
                reason: "bit field width",
            });
        }
        let end = self
            .bit_position
            .checked_add(usize::from(count))
            .ok_or(Jbig2Error::Overflow("bit position"))?;
        if end > self.input.len().saturating_mul(8) {
            return Err(Jbig2Error::Truncated {
                location: self.location(),
                needed: end.div_ceil(8).saturating_sub(self.input.len()),
            });
        }
        let mut value = 0_u32;
        for _ in 0..count {
            value = (value << 1) | u32::from(self.bit()?);
        }
        Ok(value)
    }

    /// Align to a byte boundary, then borrow and skip `count` whole bytes.
    ///
    /// A nested arithmetic body inside a Huffman payload is bounded this way,
    /// so it cannot read past its declared size and the outer cursor resumes
    /// exactly after it.
    pub(crate) fn take_aligned_bytes(&mut self, count: usize) -> Result<&'a [u8]> {
        self.align_to_byte()?;
        let start = self.bit_position / 8;
        let end = start
            .checked_add(count)
            .ok_or(Jbig2Error::Overflow("nested body extent"))?;
        let bytes = self
            .input
            .get(start..end)
            .ok_or_else(|| Jbig2Error::Truncated {
                location: self.location(),
                needed: end.saturating_sub(self.input.len()),
            })?;
        self.bit_position = end
            .checked_mul(8)
            .ok_or(Jbig2Error::Overflow("bit position"))?;
        Ok(bytes)
    }

    /// Resume at an absolute bit offset established by an earlier read.
    pub(crate) fn seek_to_bit(&mut self, bit_position: usize) {
        self.bit_position = bit_position;
    }

    /// Borrow the whole bytes spanning an earlier bit range of this input.
    ///
    /// Used to capture a body that must be re-read later; the caller keeps the
    /// start bit's offset within the first byte.
    pub(crate) fn slice_bits(self, start_bit: usize, end_bit: usize) -> Result<&'a [u8]> {
        let start = start_bit / 8;
        let end = end_bit.div_ceil(8);
        if end < start {
            return Err(Jbig2Error::InvalidResource("captured body extent"));
        }
        self.input
            .get(start..end)
            .ok_or_else(|| Jbig2Error::Truncated {
                location: self.location(),
                needed: end.saturating_sub(self.input.len()),
            })
    }

    /// Discard bits up to the next byte boundary.
    pub(crate) fn align_to_byte(&mut self) -> Result<()> {
        let remainder = self.bit_position % 8;
        if remainder == 0 {
            return Ok(());
        }
        self.bit_position = self
            .bit_position
            .checked_add(8_usize.saturating_sub(remainder))
            .ok_or(Jbig2Error::Overflow("bit position"))?;
        Ok(())
    }
}
