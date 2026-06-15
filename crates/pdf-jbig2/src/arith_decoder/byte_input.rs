//! Byte input and marker handling for the JBIG2 arithmetic decoder.
//!
//! ITU-T T.88 / ISO/IEC 14492 Annex A.1 defines the `BYTEIN` procedure and
//! the special handling of `0xff` bytes so marker bytes are not consumed as
//! arithmetic-coded data.

use crate::arith_decoder::decoder::JBig2ArithDecoder;
use pdf_utils::BitReader;

/// Fallback byte used by Annex A.1 style peeking after the segment boundary.
pub(super) const ARITHMETIC_BYTE_FALLBACK: u8 = 0xff;

/// Annex A.1 marker threshold for the byte following `0xff`.
const MARKER_BYTE_THRESHOLD: u8 = 0x8f;

/// Annex A.1 code-register adjustment after a non-`0xff` byte.
const NORMAL_BYTE_CODE_OFFSET: u32 = 0xff00;

/// Annex A.1 code-register adjustment after a stuffed byte.
const STUFFED_BYTE_CODE_OFFSET: u32 = 0xfe00;

/// Annex A.1 bit count loaded after a normal byte.
const NORMAL_BYTE_BIT_COUNT: u32 = 8;

/// Annex A.1 bit count loaded after a byte following `0xff`.
const STUFFED_BYTE_BIT_COUNT: u32 = 7;

/// Annex A.1 code-register shift for a normal byte.
const NORMAL_BYTE_CODE_SHIFT: u32 = 8;

/// Annex A.1 code-register shift for a byte following `0xff`.
const STUFFED_BYTE_CODE_SHIFT: u32 = 9;

impl JBig2ArithDecoder<'_, '_> {
    /// Return the current byte, respecting the arithmetic segment byte limit.
    pub(super) fn peek_byte_or(
        stream: &BitReader<'_>,
        byte_limit: Option<usize>,
        fallback: u8,
    ) -> u8 {
        if byte_limit.is_some_and(|limit| stream.byte_pos() >= limit) {
            fallback
        } else {
            stream.peek_byte_or(fallback)
        }
    }

    /// Return the next byte, respecting the arithmetic segment byte limit.
    pub(super) fn peek_next_byte_or(
        stream: &BitReader<'_>,
        byte_limit: Option<usize>,
        fallback: u8,
    ) -> u8 {
        let next_byte_pos = stream.byte_pos().saturating_add(1);
        if byte_limit.is_some_and(|limit| next_byte_pos >= limit) {
            fallback
        } else {
            stream.peek_next_byte_or(fallback)
        }
    }

    /// Return whether the current byte position contains a real input byte.
    pub(super) fn has_current_byte(stream: &BitReader<'_>, byte_limit: Option<usize>) -> bool {
        byte_limit.is_none_or(|limit| stream.byte_pos() < limit)
            && stream.remaining_from_byte_len(1).is_some()
    }

    /// Return whether Annex A.1 byte input has reached the stream or segment end.
    #[cfg(test)]
    pub(super) fn stream_exhausted(&self) -> bool {
        self.stream.exhausted()
            || self
                .byte_limit
                .is_some_and(|limit| self.stream.byte_pos() >= limit)
    }

    /// Execute the Annex A.1 `BYTEIN` procedure.
    pub(super) fn byte_in(&mut self) {
        if self.current_byte == ARITHMETIC_BYTE_FALLBACK {
            self.byte_in_after_ff();
        } else {
            self.byte_in_regular();
        }
    }

    /// Handle Annex A.1 byte input when the previous byte was `0xff`.
    fn byte_in_after_ff(&mut self) {
        let next = Self::peek_next_byte_or(self.stream, self.byte_limit, ARITHMETIC_BYTE_FALLBACK);
        if next > MARKER_BYTE_THRESHOLD {
            self.bit_count = NORMAL_BYTE_BIT_COUNT;
        } else {
            self.stream.advance_bytes(1);
            self.current_byte = next;
            self.code = self
                .code
                .wrapping_add(STUFFED_BYTE_CODE_OFFSET)
                .wrapping_sub(u32::from(self.current_byte) << STUFFED_BYTE_CODE_SHIFT);
            self.bit_count = STUFFED_BYTE_BIT_COUNT;
        }
    }

    /// Handle Annex A.1 byte input when the previous byte was ordinary data.
    fn byte_in_regular(&mut self) {
        self.stream.advance_bytes(1);
        self.current_byte =
            Self::peek_byte_or(self.stream, self.byte_limit, ARITHMETIC_BYTE_FALLBACK);
        self.code = self
            .code
            .wrapping_add(NORMAL_BYTE_CODE_OFFSET)
            .wrapping_sub(u32::from(self.current_byte) << NORMAL_BYTE_CODE_SHIFT);
        self.bit_count = NORMAL_BYTE_BIT_COUNT;
    }
}

#[cfg(test)]
mod tests {
    use crate::arith_decoder::JBig2ArithDecoder;
    use pdf_utils::BitReader;

    #[test]
    fn byte_limit_peeking_falls_back_at_segment_end() {
        let data = [0x12u8, 0x34u8];
        let mut reader = BitReader::new(&data);
        reader.advance_bytes(1);

        assert_eq!(
            JBig2ArithDecoder::peek_byte_or(&reader, Some(1), 0xff),
            0xff
        );
        assert_eq!(
            JBig2ArithDecoder::peek_next_byte_or(&reader, Some(2), 0xff),
            0xff
        );
    }

    #[test]
    fn new_until_keeps_decoder_open_at_segment_boundary() {
        let data = [0x12u8];
        let mut reader = BitReader::new(&data);
        let decoder = JBig2ArithDecoder::new_until(&mut reader, 1).expect("decoder");

        assert!(!decoder.complete);
        assert!(decoder.stream_exhausted());
    }
}
