/// Reads bits MSB-first from a byte slice.
#[derive(Debug, Clone)]
pub struct BitReader<'a> {
    src: &'a [u8],
    bit_pos: usize,
}

impl<'a> BitReader<'a> {
    /// Creates a reader positioned at the first bit of `src`.
    pub fn new(src: &'a [u8]) -> Self {
        Self { src, bit_pos: 0 }
    }

    /// Read the next bit and advance. Returns `None` when exhausted.
    #[allow(clippy::arithmetic_side_effects)]
    pub fn next_bit(&mut self) -> Option<bool> {
        let pos = self.bit_pos;
        let byte = self.src.get(pos / 8)?;
        self.bit_pos += 1;
        Some((byte >> (7 - pos % 8)) & 1 != 0)
    }

    /// Returns the current bit position from the start of the slice.
    pub fn pos(&self) -> usize {
        self.bit_pos
    }

    /// Repositions the reader to an absolute bit offset.
    ///
    /// Positions beyond the logical end of the slice are allowed. Subsequent
    /// bit reads return `None` and byte-aligned reads return truncation
    /// errors until the position is moved back in bounds.
    pub fn set_pos(&mut self, pos: usize) {
        self.bit_pos = pos;
    }

    /// Returns the current byte index, rounded down from the bit position.
    pub fn byte_pos(&self) -> usize {
        self.bit_pos / 8
    }

    /// Returns the current bit offset within the current byte.
    pub fn bit_offset(&self) -> usize {
        self.bit_pos % 8
    }

    /// Repositions the reader to `byte_pos`, preserving the intra-byte offset.
    pub fn set_byte_pos_preserving_offset(&mut self, byte_pos: usize) {
        self.bit_pos = byte_pos.saturating_mul(8).saturating_add(self.bit_offset());
    }

    /// Advances the byte index by `bytes`, preserving the intra-byte offset.
    pub fn advance_bytes(&mut self, bytes: usize) {
        self.bit_pos = self.bit_pos.saturating_add(bytes.saturating_mul(8));
    }

    /// Returns `true` when no more bits can be read.
    pub fn exhausted(&self) -> bool {
        self.bit_pos >= self.src.len().saturating_mul(8)
    }

    /// Returns the number of whole bytes remaining starting at the current
    /// byte index.
    pub fn remaining_bytes(&self) -> usize {
        self.src.len().saturating_sub(self.byte_pos())
    }

    /// Returns the current byte, or `fallback` when the byte index is out of
    /// bounds.
    pub fn peek_byte_or(&self, fallback: u8) -> u8 {
        self.src.get(self.byte_pos()).copied().unwrap_or(fallback)
    }

    /// Returns the next byte after the current one, or `fallback` when it does
    /// not exist.
    pub fn peek_next_byte_or(&self, fallback: u8) -> u8 {
        self.src
            .get(self.byte_pos().saturating_add(1))
            .copied()
            .unwrap_or(fallback)
    }

    /// Returns the remaining slice starting at the current byte index.
    pub fn remaining_from_byte(&self) -> Option<&'a [u8]> {
        self.src.get(self.byte_pos()..)
    }

    /// Returns `byte_len` bytes starting at the current byte index.
    ///
    /// The current byte index is rounded down from the current bit position.
    /// The reader position is not changed.
    pub fn remaining_from_byte_len(&self, byte_len: usize) -> Option<&'a [u8]> {
        self.src
            .get(self.byte_pos()..self.byte_pos().saturating_add(byte_len))
    }

    /// Returns `byte_len` bytes starting at the current byte index and advances
    /// by that many bytes.
    ///
    /// The current byte index is rounded down from the current bit position.
    /// If the requested range is unavailable, the reader position is not
    /// changed.
    pub fn take_from_byte_len(&mut self, byte_len: usize) -> Option<&'a [u8]> {
        let bytes = self.remaining_from_byte_len(byte_len)?;
        self.advance_bytes(byte_len);
        Some(bytes)
    }

    /// Returns the byte slice from the current byte index up to `end_byte_pos`.
    ///
    /// The current byte index is rounded down from the current bit position.
    /// If `end_byte_pos` is before the current byte index, this returns an
    /// empty slice. The reader position is not changed.
    pub fn remaining_from_byte_until(&self, end_byte_pos: usize) -> Option<&'a [u8]> {
        let byte_len = end_byte_pos.saturating_sub(self.byte_pos());
        self.remaining_from_byte_len(byte_len)
    }

    /// Advance past the next `n` bits, discarding their values.
    ///
    /// If the stream is exhausted before `n` bits have been consumed, reading
    /// stops silently at the end — identical to calling [`next_bit`] `n` times
    /// and ignoring the results.
    pub fn skip_bits(&mut self, n: usize) {
        for _ in 0..n {
            self.next_bit();
        }
    }

    /// Read `n` bits MSB-first and return them as a `u16`.
    ///
    /// Returns `None` if there are fewer than `n` bits remaining. `n` must be
    /// at most 16.
    #[allow(clippy::arithmetic_side_effects)]
    pub fn read_bits(&mut self, n: u8) -> Option<u16> {
        let total_bits = self.src.len().saturating_mul(8);
        if self.bit_pos.saturating_add(usize::from(n)) > total_bits {
            return None;
        }
        let mut value: u16 = 0;
        for _ in 0..n {
            value <<= 1;
            if self.next_bit()? {
                value |= 1;
            }
        }
        Some(value)
    }

    /// Reads a big-endian byte-aligned integer from the current byte index.
    ///
    /// This is intended for byte-aligned consumers. The read advances by one
    /// byte regardless of the current bit offset. The caller selects the
    /// destination type `T`; the read returns
    /// [`BitReaderError::Overflow`] if the value cannot be
    /// represented in `T`, and [`BitReaderError::Truncated`] if the
    /// read runs past the end of the slice.
    pub fn try_read_u8<T>(&mut self) -> Result<T, crate::error::BitReaderError>
    where
        T: TryFrom<u8>,
    {
        let byte = self
            .src
            .get(self.byte_pos())
            .copied()
            .ok_or(crate::error::BitReaderError::Truncated("byte-aligned read"))?;
        self.advance_bytes(1);
        T::try_from(byte)
            .map_err(|_| crate::error::BitReaderError::Overflow("integer conversion overflow"))
    }

    /// Reads a byte-aligned signed 8-bit value from the current byte index.
    ///
    /// This is intended for byte-aligned consumers. The read advances by one
    /// byte regardless of the current bit offset. The value is interpreted as
    /// two's-complement `i8` without any additional conversion. The read
    /// returns [`BitReaderError::Truncated`] if it runs past the
    /// end of the slice.
    pub fn try_read_i8(&mut self) -> Result<i8, crate::error::BitReaderError> {
        let byte = self
            .src
            .get(self.byte_pos())
            .copied()
            .ok_or(crate::error::BitReaderError::Truncated("byte-aligned read"))?;
        self.advance_bytes(1);
        Ok(i8::from_ne_bytes([byte]))
    }

    /// Reads a big-endian 16-bit value from the current byte index.
    ///
    /// This is intended for byte-aligned consumers. The read advances by two
    /// bytes regardless of the current bit offset. The caller selects the
    /// destination type `T`; the read returns
    /// [`BitReaderError::Overflow`] if the value cannot be
    /// represented in `T`, and [`BitReaderError::Truncated`] if the
    /// read runs past the end of the slice.
    pub fn try_read_u16_be<T>(&mut self) -> Result<T, crate::error::BitReaderError>
    where
        T: TryFrom<u16>,
    {
        let bytes = self
            .src
            .get(self.byte_pos()..self.byte_pos().saturating_add(2))
            .ok_or(crate::error::BitReaderError::Truncated("byte-aligned read"))?;
        let bytes: [u8; 2] = bytes
            .try_into()
            .map_err(|_| crate::error::BitReaderError::Truncated("byte-aligned read"))?;
        let value = u16::from_be_bytes(bytes);
        self.advance_bytes(2);
        T::try_from(value)
            .map_err(|_| crate::error::BitReaderError::Overflow("integer conversion overflow"))
    }

    /// Reads a big-endian 32-bit value from the current byte index.
    ///
    /// This is intended for byte-aligned consumers. The read advances by four
    /// bytes regardless of the current bit offset. The caller selects the
    /// destination type `T`; the read returns
    /// [`BitReaderError::Overflow`] if the value cannot be
    /// represented in `T`, and [`BitReaderError::Truncated`] if the
    /// read runs past the end of the slice.
    pub fn try_read_u32_be<T>(&mut self) -> Result<T, crate::error::BitReaderError>
    where
        T: TryFrom<u32>,
    {
        let bytes = self
            .src
            .get(self.byte_pos()..self.byte_pos().saturating_add(4))
            .ok_or(crate::error::BitReaderError::Truncated("byte-aligned read"))?;
        let bytes: [u8; 4] = bytes
            .try_into()
            .map_err(|_| crate::error::BitReaderError::Truncated("byte-aligned read"))?;
        let value = u32::from_be_bytes(bytes);
        self.advance_bytes(4);
        T::try_from(value)
            .map_err(|_| crate::error::BitReaderError::Overflow("integer conversion overflow"))
    }

    /// Reads a big-endian signed 32-bit value from the current byte index.
    ///
    /// This is intended for byte-aligned consumers. The read advances by four
    /// bytes regardless of the current bit offset.
    pub fn try_read_i32_be(&mut self) -> Result<i32, crate::error::BitReaderError> {
        let value = self.try_read_u32_be::<u32>()?;
        if value <= 0x7fff_ffff {
            return i32::try_from(value).map_err(|_| {
                crate::error::BitReaderError::Overflow("integer conversion overflow")
            });
        }
        if value == 0x8000_0000 {
            return Ok(i32::MIN);
        }

        let magnitude = (!value)
            .checked_add(1)
            .ok_or(crate::error::BitReaderError::Overflow(
                "integer conversion overflow",
            ))?;
        i32::try_from(magnitude)
            .map_err(|_| crate::error::BitReaderError::Overflow("integer conversion overflow"))?
            .checked_neg()
            .ok_or(crate::error::BitReaderError::Overflow(
                "integer conversion overflow",
            ))
    }

    /// Advance to the next byte boundary, but only if all padding bits are 0.
    /// Returns `true` if alignment happened, `false` if a non-zero pad bit was
    /// found (the caller should disable byte-alignment for remaining rows).
    #[allow(clippy::arithmetic_side_effects)]
    pub fn try_align_to_byte(&mut self) -> bool {
        let cur = self.bit_pos;
        let aligned = (cur + 7) & !7;
        for p in cur..aligned {
            let is_set = self
                .src
                .get(p / 8)
                .is_some_and(|&b| (b >> (7 - p % 8)) & 1 != 0);
            if is_set {
                return false;
            }
        }
        self.bit_pos = aligned;
        true
    }

    /// Advances to the next byte boundary by discarding any remaining bits in
    /// the current byte.
    pub fn align_to_byte_boundary(&mut self) {
        let rem = self.bit_offset();
        if rem != 0 {
            self.skip_bits(8usize.saturating_sub(rem));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bit_reader_skip_bits_advances_position() {
        let data = [0b1111_0000u8, 0b0000_1111u8];
        let mut r = BitReader::new(&data);
        r.skip_bits(4);
        assert_eq!(r.pos(), 4);
        // next bit should be the 5th bit of first byte (0 in 0b1111_0000)
        assert_eq!(r.next_bit(), Some(false));
    }

    #[test]
    fn bit_reader_skip_bits_past_end_is_safe() {
        let data = [0xffu8];
        let mut r = BitReader::new(&data);
        r.skip_bits(100); // should not panic
        assert!(r.exhausted());
    }

    #[test]
    fn read_bits_returns_msb_first_value() {
        // 0b1010_0110 = 0xA6
        let data = [0xA6u8];
        let mut r = BitReader::new(&data);
        // Read 4 bits: 1010 = 10
        assert_eq!(r.read_bits(4), Some(0b1010));
        // Read 4 bits: 0110 = 6
        assert_eq!(r.read_bits(4), Some(0b0110));
    }

    #[test]
    fn read_bits_across_byte_boundary() {
        let data = [0b1111_0000u8, 0b1010_1010u8];
        let mut r = BitReader::new(&data);
        r.skip_bits(4);
        // Read 8 bits spanning bytes: 0000_1010 = 0x0A
        assert_eq!(r.read_bits(8), Some(0b0000_1010));
    }

    #[test]
    fn read_bits_returns_none_when_exhausted() {
        let data = [0xFFu8];
        let mut r = BitReader::new(&data);
        assert_eq!(r.read_bits(9), None);
    }

    #[test]
    fn read_bits_9_bit_code() {
        // Two bytes: 0b1_0000_0001 0xxxxxxx
        // Code 0x101 = 257 in 9 bits MSB-first
        // Byte 0: 1000_0000  Byte 1: 1xxxxxxx
        let data = [0b1000_0000u8, 0b1000_0000u8];
        let mut r = BitReader::new(&data);
        assert_eq!(r.read_bits(9), Some(0b1_0000_0001)); // 257
    }

    #[test]
    fn byte_position_helpers_preserve_offset() {
        let data = [0x12u8, 0x34u8, 0x56u8];
        let mut r = BitReader::new(&data);
        r.skip_bits(3);
        assert_eq!(r.byte_pos(), 0);
        assert_eq!(r.bit_offset(), 3);
        r.set_byte_pos_preserving_offset(2);
        assert_eq!(r.pos(), 19);
        assert_eq!(r.byte_pos(), 2);
        assert_eq!(r.bit_offset(), 3);
    }

    #[test]
    fn byte_reads_and_peeks_are_big_endian() {
        let data = [0x12u8, 0x34u8, 0x56u8, 0x78u8];
        let mut r = BitReader::new(&data);
        assert_eq!(r.peek_byte_or(0xff), 0x12);
        assert_eq!(r.peek_next_byte_or(0xff), 0x34);
        assert_eq!(r.try_read_u8::<u8>(), Ok(0x12));
        assert_eq!(r.try_read_u16_be::<u16>(), Ok(0x3456));
        assert_eq!(r.try_read_u8::<u8>(), Ok(0x78));
        assert_eq!(r.peek_byte_or(0xff), 0xff);
    }

    #[test]
    fn signed_byte_reads_preserve_twos_complement_values() {
        let data = [0x00u8, 0x7fu8, 0x80u8, 0xffu8];
        let mut r = BitReader::new(&data);

        assert_eq!(r.try_read_i8(), Ok(0));
        assert_eq!(r.try_read_i8(), Ok(127));
        assert_eq!(r.try_read_i8(), Ok(-128));
        assert_eq!(r.try_read_i8(), Ok(-1));
    }

    #[test]
    fn signed_byte_reads_advance_by_one_byte() {
        let data = [0x80u8, 0x7fu8];
        let mut r = BitReader::new(&data);
        assert_eq!(r.try_read_i8(), Ok(-128));
        assert_eq!(r.byte_pos(), 1);
        assert_eq!(r.try_read_i8(), Ok(127));
        assert_eq!(r.byte_pos(), 2);
    }

    #[test]
    fn signed_byte_reads_return_truncation_errors() {
        let data = [0x12u8];
        let mut r = BitReader::new(&data);
        assert_eq!(r.try_read_i8(), Ok(0x12));
        assert_eq!(
            r.try_read_i8(),
            Err(crate::error::BitReaderError::Truncated("byte-aligned read"))
        );
    }

    #[test]
    fn byte_reads_return_truncation_errors() {
        let data = [0x12u8];
        let mut r = BitReader::new(&data);
        assert_eq!(
            r.try_read_u16_be::<u16>(),
            Err(crate::error::BitReaderError::Truncated("byte-aligned read"))
        );
    }

    #[test]
    fn typed_byte_reads_convert_values() {
        let data = [0x00u8, 0x00u8, 0x00u8, 0x10u8];
        let mut r = BitReader::new(&data);
        assert_eq!(r.try_read_u32_be::<usize>(), Ok(16));
    }

    #[test]
    fn typed_byte_reads_report_overflow() {
        let data = [0x00u8, 0x01u8, 0x00u8, 0x00u8];
        let mut r = BitReader::new(&data);
        assert_eq!(
            r.try_read_u32_be::<u16>(),
            Err(crate::error::BitReaderError::Overflow(
                "integer conversion overflow"
            ))
        );
    }

    #[test]
    fn signed_i32_byte_reads_preserve_twos_complement_values() {
        let data = [
            0x00, 0x00, 0x00, 0x00, // 0
            0x7f, 0xff, 0xff, 0xff, // i32::MAX
            0x80, 0x00, 0x00, 0x00, // i32::MIN
            0xff, 0xff, 0xff, 0xff, // -1
            0xff, 0xff, 0xff, 0xf0, // -16
        ];
        let mut r = BitReader::new(&data);

        assert_eq!(r.try_read_i32_be(), Ok(0));
        assert_eq!(r.try_read_i32_be(), Ok(i32::MAX));
        assert_eq!(r.try_read_i32_be(), Ok(i32::MIN));
        assert_eq!(r.try_read_i32_be(), Ok(-1));
        assert_eq!(r.try_read_i32_be(), Ok(-16));
    }

    #[test]
    fn remaining_from_byte_starts_at_current_byte_index() {
        let data = [0x12u8, 0x34u8, 0x56u8];
        let mut r = BitReader::new(&data);
        r.skip_bits(9);
        assert_eq!(r.remaining_from_byte(), Some(&data[1..]));
    }

    #[test]
    fn remaining_from_byte_until_returns_range_up_to_end_byte_pos() {
        let data = [0x12u8, 0x34u8, 0x56u8, 0x78u8];
        let mut r = BitReader::new(&data);
        r.skip_bits(4);
        let pos = r.pos();
        let offset = r.bit_offset();
        assert_eq!(r.remaining_from_byte_until(3), Some(&data[0..3]));
        assert_eq!(r.pos(), pos);
        assert_eq!(r.bit_offset(), offset);
    }

    #[test]
    fn remaining_from_byte_until_returns_empty_slice_when_end_is_before_current_byte() {
        let data = [0x12u8, 0x34u8];
        let mut r = BitReader::new(&data);
        r.advance_bytes(1);
        assert_eq!(r.remaining_from_byte_until(0), Some(&data[1..1]));
    }

    #[test]
    fn remaining_from_byte_until_returns_none_when_end_exceeds_input() {
        let data = [0x12u8, 0x34u8];
        let r = BitReader::new(&data);
        assert_eq!(r.remaining_from_byte_until(3), None);
    }

    #[test]
    fn remaining_from_byte_len_returns_exact_length_and_preserves_position() {
        let data = [0x12u8, 0x34u8, 0x56u8];
        let mut r = BitReader::new(&data);
        r.skip_bits(5);
        let pos = r.pos();
        let offset = r.bit_offset();
        assert_eq!(r.remaining_from_byte_len(2), Some(&data[0..2]));
        assert_eq!(r.pos(), pos);
        assert_eq!(r.bit_offset(), offset);
    }

    #[test]
    fn remaining_from_byte_len_returns_empty_slice_for_zero_length() {
        let data = [0x12u8, 0x34u8];
        let r = BitReader::new(&data);
        assert_eq!(r.remaining_from_byte_len(0), Some(&data[0..0]));
    }

    #[test]
    fn remaining_from_byte_len_returns_none_when_length_exceeds_input() {
        let data = [0x12u8, 0x34u8];
        let r = BitReader::new(&data);
        assert_eq!(r.remaining_from_byte_len(3), None);
    }

    #[test]
    fn take_from_byte_len_returns_exact_length_and_advances_position() {
        let data = [0x12u8, 0x34u8, 0x56u8];
        let mut r = BitReader::new(&data);
        r.skip_bits(5);

        assert_eq!(r.take_from_byte_len(2), Some(&data[0..2]));
        assert_eq!(r.byte_pos(), 2);
        assert_eq!(r.bit_offset(), 5);
    }

    #[test]
    fn take_from_byte_len_returns_empty_slice_for_zero_length() {
        let data = [0x12u8, 0x34u8];
        let mut r = BitReader::new(&data);
        r.skip_bits(5);

        assert_eq!(r.take_from_byte_len(0), Some(&data[0..0]));
        assert_eq!(r.byte_pos(), 0);
        assert_eq!(r.bit_offset(), 5);
    }

    #[test]
    fn take_from_byte_len_returns_none_and_preserves_position_when_length_exceeds_input() {
        let data = [0x12u8, 0x34u8];
        let mut r = BitReader::new(&data);
        r.skip_bits(5);

        assert_eq!(r.take_from_byte_len(3), None);
        assert_eq!(r.byte_pos(), 0);
        assert_eq!(r.bit_offset(), 5);
    }

    #[test]
    fn set_byte_pos_preserving_offset_matches_jbig2_behavior() {
        let data = [0x12u8, 0x34u8, 0x56u8];
        let mut r = BitReader::new(&data);
        assert_eq!(r.next_bit(), Some(false));
        assert_eq!(r.pos(), 1);
        r.set_byte_pos_preserving_offset(2);
        assert_eq!(r.byte_pos(), 2);
        assert_eq!(r.pos(), 17);
    }

    #[test]
    fn arithmetic_peeks_fall_back_to_ff_past_end() {
        let data = [0x12u8];
        let mut r = BitReader::new(&data);
        assert_eq!(r.peek_byte_or(0xff), 0x12);
        assert_eq!(r.peek_next_byte_or(0xff), 0xff);
        r.advance_bytes(1);
        assert_eq!(r.peek_byte_or(0xff), 0xff);
        assert_eq!(r.peek_next_byte_or(0xff), 0xff);
    }

    #[test]
    fn align_to_byte_boundary_advances_to_next_byte() {
        let data = [0b1111_0000u8, 0b1010_1010u8];
        let mut r = BitReader::new(&data);
        r.skip_bits(3);
        r.align_to_byte_boundary();
        assert_eq!(r.pos(), 8);
        assert_eq!(r.bit_offset(), 0);
        assert_eq!(r.byte_pos(), 1);
        assert_eq!(r.next_bit(), Some(true));
    }

    #[test]
    fn align_to_byte_boundary_is_noop_when_already_aligned() {
        let data = [0b1111_0000u8];
        let mut r = BitReader::new(&data);
        r.align_to_byte_boundary();
        assert_eq!(r.pos(), 0);
        assert_eq!(r.bit_offset(), 0);
    }
}
