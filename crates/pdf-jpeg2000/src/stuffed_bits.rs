//! Bit reading with the JPEG 2000 stuffing rule.
//!
//! Two places in Part 1 read a bit stream that must not contain a marker: the
//! packet header of Annex B.10.1, and the raw code-block segments produced by
//! the selective arithmetic bypass of Annex D.7. Both use the same rule, read
//! most-significant bit first: whenever a byte is `0xff`, the next byte
//! carries a stuffed zero in its most significant bit and contributes only
//! seven bits. `pdf_utils::BitReader` has no such rule, so both use this
//! reader instead.
//!
//! The bytes may be spread over several slices — a packet header across PPM
//! or PPT segments, a codeword segment across the tile-parts of successive
//! layers — so the reader draws them from a [`HeaderSource`] rather than from
//! one slice.

use crate::{Jpeg2000Error, offset_site::OffsetSite};

/// Bits contributed by an ordinary header byte.
const FULL_BITS: u8 = 8;
/// Bits contributed by the byte following `0xff`, whose top bit is stuffed.
const STUFFED_BITS: u8 = 7;
/// Header byte after which the next byte carries a stuffed bit.
const STUFFING_BYTE: u8 = 0xff;
/// Largest value the byte following `0xff` may take.
const MAX_STUFFED_BYTE: u8 = 0x7f;

/// A byte supply for a stuffed bit stream.
///
/// Implementations report how many bytes were consumed so a caller can align
/// the compressed body that follows the header.
pub(crate) trait HeaderSource {
    /// Returns the next header byte, or `None` at the end of the supply.
    fn next_byte(&mut self) -> Option<u8>;

    /// Returns the number of bytes taken from the supply so far.
    fn consumed(&self) -> usize;
}

/// Most-significant-bit-first reader applying the JPEG 2000 stuffing rule.
///
/// The reader owns its byte supply so that its bit position can outlive a
/// single coding pass, which a raw codeword segment requires.
#[derive(Clone, Copy, Debug)]
pub(crate) struct StuffedBitReader<S> {
    source: S,
    site: OffsetSite,
    current: u8,
    bits_left: u8,
}

impl<S: HeaderSource> StuffedBitReader<S> {
    /// Starts reading a stuffed bit stream from a byte supply.
    ///
    /// `offset` is the codestream position of the first byte and is used only
    /// to report truncation.
    pub(crate) fn new(source: S, offset: usize) -> Self {
        Self {
            source,
            site: OffsetSite::new(offset),
            current: 0,
            bits_left: 0,
        }
    }

    /// Returns the byte supply, for a caller that reads past the bit stream.
    pub(crate) fn source_mut(&mut self) -> &mut S {
        &mut self.source
    }

    /// Returns the number of bytes consumed so far.
    pub(crate) fn consumed(&self) -> usize {
        self.source.consumed()
    }

    /// Reads one header bit.
    ///
    /// # Errors
    ///
    /// Returns `Truncated` at the end of the supply and `InvalidMarker` when a
    /// stuffed byte carries a set most significant bit, which would make the
    /// header contain a marker.
    pub(crate) fn read_bit(&mut self) -> Result<bool, Jpeg2000Error> {
        if self.bits_left == 0 {
            self.load()?;
        }
        let remaining = self.bits_left.saturating_sub(1);
        self.bits_left = remaining;
        Ok(self.current >> remaining & 1 == 1)
    }

    /// Reads `count` header bits as an unsigned value, most significant first.
    ///
    /// # Errors
    ///
    /// Returns `Truncated` or `InvalidMarker` as [`StuffedBitReader::read_bit`]
    /// does, and `Overflow` if more than 32 bits are requested.
    pub(crate) fn read_bits(&mut self, count: u32) -> Result<u32, Jpeg2000Error> {
        if count > u32::BITS {
            return Err(Jpeg2000Error::Overflow {
                context: "packet header field width",
            });
        }
        let mut value = 0u32;
        for _ in 0..count {
            value = value.checked_shl(1).ok_or(Jpeg2000Error::Overflow {
                context: "packet header field width",
            })? | u32::from(self.read_bit()?);
        }
        Ok(value)
    }

    /// Reads a unary-coded value terminated by a zero bit.
    ///
    /// # Errors
    ///
    /// Returns `Truncated` at the end of the supply, or `Overflow` when the
    /// run exceeds `limit`, which keeps a corrupt header from spinning.
    pub(crate) fn read_unary(&mut self, limit: u32) -> Result<u32, Jpeg2000Error> {
        let mut count = 0u32;
        while self.read_bit()? {
            count = count.checked_add(1).ok_or(Jpeg2000Error::Overflow {
                context: "packet header unary code",
            })?;
            if count > limit {
                return Err(Jpeg2000Error::Overflow {
                    context: "packet header unary code",
                });
            }
        }
        Ok(count)
    }

    /// Ends the header, discarding padding and any trailing stuffed byte.
    ///
    /// # Errors
    ///
    /// Returns `Truncated` if the trailing stuffed byte is missing.
    pub(crate) fn align(&mut self) -> Result<(), Jpeg2000Error> {
        if self.current == STUFFING_BYTE {
            self.load()?;
        }
        self.bits_left = 0;
        self.current = 0;
        Ok(())
    }

    /// Loads the next header byte, applying the stuffing rule.
    fn load(&mut self) -> Result<(), Jpeg2000Error> {
        let stuffed = self.current == STUFFING_BYTE;
        let byte = self
            .source
            .next_byte()
            .ok_or_else(|| self.site.truncated("packet header"))?;
        let marker_site = self.site.marker(u16::from_be_bytes([STUFFING_BYTE, byte]));
        if stuffed && byte > MAX_STUFFED_BYTE {
            return marker_site.reject_invalid("marker inside a packet header");
        }
        self.current = byte;
        self.bits_left = if stuffed { STUFFED_BITS } else { FULL_BITS };
        Ok(())
    }
}
