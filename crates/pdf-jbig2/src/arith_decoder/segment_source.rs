//! The byte supply the JBIG2 arithmetic decoder reads from.
//!
//! T.88 arithmetic data sits inside a segment of a larger JBIG2 stream, so the
//! source is the shared segment reader bounded by the segment's end byte.
//! Reads at or past that end report no byte, and the MQ coder substitutes the
//! `0xff` padding Annex A.1 prescribes.

use pdf_mq_coder::MqByteSource;
use pdf_utils::BitReader;

/// Bytes the arithmetic coder inspects without consuming them.
const LOOKAHEAD_BYTES: usize = 2;

/// A borrowed view of one JBIG2 segment's arithmetic bytes.
#[derive(Debug)]
pub(super) struct SegmentSource<'stream, 'data> {
    stream: &'stream mut BitReader<'data>,
    byte_limit: Option<usize>,
}

impl<'stream, 'data> SegmentSource<'stream, 'data> {
    /// Borrows a segment reader, optionally stopping at an end byte.
    pub(super) fn new(stream: &'stream mut BitReader<'data>, byte_limit: Option<usize>) -> Self {
        Self { stream, byte_limit }
    }

    /// Returns the borrowed segment reader.
    pub(super) fn into_stream(self) -> &'stream mut BitReader<'data> {
        self.stream
    }

    /// Returns whether a byte position is inside the segment.
    fn within_segment(&self, position: usize) -> bool {
        self.byte_limit.is_none_or(|limit| position < limit)
    }
}

impl MqByteSource for SegmentSource<'_, '_> {
    fn peek(&self) -> Option<u8> {
        if !self.within_segment(self.stream.byte_pos()) {
            return None;
        }
        self.stream.remaining_from_byte_len(1)?.first().copied()
    }

    fn peek_next(&self) -> Option<u8> {
        if !self.within_segment(self.stream.byte_pos().checked_add(1)?) {
            return None;
        }
        self.stream
            .remaining_from_byte_len(LOOKAHEAD_BYTES)?
            .get(1)
            .copied()
    }

    fn advance(&mut self) {
        self.stream.advance_bytes(1);
    }
}

#[cfg(test)]
mod tests {
    use super::SegmentSource;
    use pdf_mq_coder::MqByteSource;
    use pdf_utils::BitReader;

    #[test]
    fn peeking_stops_at_the_segment_end() {
        let data = [0x12u8, 0x34u8];
        let mut reader = BitReader::new(&data);
        reader.advance_bytes(1);

        assert_eq!(SegmentSource::new(&mut reader, Some(1)).peek(), None);
        assert_eq!(SegmentSource::new(&mut reader, Some(2)).peek_next(), None);
    }
}
