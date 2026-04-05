/// Reads bits MSB-first from a byte slice.
pub struct BitReader<'a> {
    src: &'a [u8],
    bit_pos: usize,
}

impl<'a> BitReader<'a> {
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

    pub fn pos(&self) -> usize {
        self.bit_pos
    }

    pub fn set_pos(&mut self, pos: usize) {
        self.bit_pos = pos;
    }

    pub fn exhausted(&self) -> bool {
        self.bit_pos >= self.src.len().saturating_mul(8)
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
}
