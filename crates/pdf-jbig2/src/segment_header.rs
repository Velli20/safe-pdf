//! JBIG2 segment header parsing.

use crate::{
    error::Jbig2Error,
    segment::{JBig2SegmentResult, ParsedSegment},
};
use bitflags::bitflags;
use pdf_utils::BitReader;

pub(crate) const REFERRED_COUNT_SHORT_FORM_SHIFT: u8 = 5;
pub(crate) const REFERRED_COUNT_LONG_FORM_MARKER: u8 = 0b111;
pub(crate) const REFERRED_COUNT_LONG_FORM_MASK: u32 = 0x1fff_ffff;
pub(crate) const REFERRED_COUNT_RETAIN_SELF_BITS: u32 = 1;
pub(crate) const BITS_PER_BYTE: u32 = 8;
pub(crate) const ONE_BYTE_SEGMENT_NUMBER_MAX: u32 = 256;
pub(crate) const TWO_BYTE_SEGMENT_NUMBER_MAX: u32 = 65_536;
pub(crate) const UNKNOWN_SEGMENT_DATA_LENGTH: u32 = u32::MAX;

bitflags! {
    /// JBIG2 segment header flags from spec section 7.2.3.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub(crate) struct SegmentHeaderFlagBits: u8 {
        /// Bits 0-5 encode the JBIG2 segment type.
        const SEGMENT_TYPE_MASK = 0b11_1111;
        /// Bit 6 selects 1-byte or 4-byte page association encoding.
        const PAGE_ASSOCIATION_SIZE = 1 << 6;
        /// Bit 7 disables retention of referred segments.
        const DEFERRED_NON_RETAIN = 1 << 7;
    }
}

impl SegmentHeaderFlagBits {
    pub(crate) fn segment_type(self) -> u8 {
        self.bits() & Self::SEGMENT_TYPE_MASK.bits()
    }

    pub(crate) fn page_association_is_32_bit(self) -> bool {
        self.contains(Self::PAGE_ASSOCIATION_SIZE)
    }
}

pub(crate) fn referred_segment_count_is_long_form(byte: u8) -> bool {
    (byte >> REFERRED_COUNT_SHORT_FORM_SHIFT) == REFERRED_COUNT_LONG_FORM_MARKER
}

impl TryFrom<&mut BitReader<'_>> for ParsedSegment {
    type Error = Jbig2Error;

    fn try_from(stream: &mut BitReader<'_>) -> Result<Self, Self::Error> {
        let number = stream.try_read_u32_be::<u32>()?;
        let flags = stream.try_read_u8::<u8>()?;
        let flag_bits = SegmentHeaderFlagBits::from_bits_retain(flags);

        let referred_count_and_retention = stream.peek_byte_or(0);
        let referred_count = if referred_segment_count_is_long_form(referred_count_and_retention) {
            let count = stream.try_read_u32_be::<u32>()? & REFERRED_COUNT_LONG_FORM_MASK;
            let retained_bytes = count
                .saturating_add(REFERRED_COUNT_RETAIN_SELF_BITS)
                .div_ceil(BITS_PER_BYTE);
            for _ in 0..retained_bytes {
                let _ = stream.try_read_u8::<u8>()?;
            }
            usize::try_from(count)
                .map_err(|_| Jbig2Error::Overflow("integer conversion overflow"))?
        } else {
            usize::from(stream.try_read_u8::<u8>()? >> REFERRED_COUNT_SHORT_FORM_SHIFT)
        };

        let segment_number_size = if number > TWO_BYTE_SEGMENT_NUMBER_MAX {
            4usize
        } else if number > ONE_BYTE_SEGMENT_NUMBER_MAX {
            2usize
        } else {
            1usize
        };
        let mut referred_to_segment_numbers = Vec::with_capacity(referred_count);
        for _ in 0..referred_count {
            let referred = match segment_number_size {
                1 => u32::from(stream.try_read_u8::<u8>()?),
                2 => u32::from(stream.try_read_u16_be::<u16>()?),
                4 => stream.try_read_u32_be::<u32>()?,
                _ => return Err(Jbig2Error::InvalidState("segment header")),
            };
            referred_to_segment_numbers.push(referred);
        }

        let page_association = if flag_bits.page_association_is_32_bit() {
            stream.try_read_u32_be::<u32>()?
        } else {
            u32::from(stream.try_read_u8::<u8>()?)
        };
        let data_length = match stream.try_read_u32_be::<u32>()? {
            UNKNOWN_SEGMENT_DATA_LENGTH => None,
            value => Some(
                usize::try_from(value)
                    .map_err(|_| Jbig2Error::Overflow("integer conversion overflow"))?,
            ),
        };

        Ok(ParsedSegment {
            number,
            flags,
            referred_to_segment_numbers,
            page_association,
            data_length,
            result: JBig2SegmentResult::None,
        })
    }
}

#[cfg(test)]
mod tests {
    use pdf_utils::BitReader;

    use crate::{
        error::Jbig2Error,
        segment::ParsedSegment,
        segment_header::{
            ONE_BYTE_SEGMENT_NUMBER_MAX, REFERRED_COUNT_LONG_FORM_MARKER,
            REFERRED_COUNT_SHORT_FORM_SHIFT, SegmentHeaderFlagBits, TWO_BYTE_SEGMENT_NUMBER_MAX,
            referred_segment_count_is_long_form,
        },
    };

    #[test]
    fn truncated_header_returns_typed_error() {
        let mut stream = BitReader::new(&[]);
        let err = ParsedSegment::try_from(&mut stream).expect_err("expected error");
        assert_eq!(err, Jbig2Error::Truncated("byte-aligned read"));
    }

    #[test]
    fn detects_long_form_referred_count() {
        assert!(referred_segment_count_is_long_form(
            REFERRED_COUNT_LONG_FORM_MARKER << REFERRED_COUNT_SHORT_FORM_SHIFT
        ));
        assert!(!referred_segment_count_is_long_form(0x00));
    }

    #[test]
    fn extracts_segment_header_flags() {
        let bits = SegmentHeaderFlagBits::from_bits_retain(
            SegmentHeaderFlagBits::PAGE_ASSOCIATION_SIZE.bits()
                | SegmentHeaderFlagBits::SEGMENT_TYPE_MASK.bits(),
        );
        assert_eq!(
            bits.segment_type(),
            SegmentHeaderFlagBits::SEGMENT_TYPE_MASK.bits()
        );
        assert!(bits.page_association_is_32_bit());
        assert!(!bits.contains(SegmentHeaderFlagBits::DEFERRED_NON_RETAIN));
    }

    fn push_u8(bytes: &mut Vec<u8>, value: u8) {
        bytes.push(value);
    }

    fn push_u16(bytes: &mut Vec<u8>, value: u16) {
        bytes.extend_from_slice(&value.to_be_bytes());
    }

    fn push_u32(bytes: &mut Vec<u8>, value: u32) {
        bytes.extend_from_slice(&value.to_be_bytes());
    }

    fn push_short_form_referred_count(bytes: &mut Vec<u8>, count: u8) {
        bytes.push(count << REFERRED_COUNT_SHORT_FORM_SHIFT);
    }

    fn push_long_form_referred_count(bytes: &mut Vec<u8>, count: u32, retention: &[u8]) {
        push_u32(
            bytes,
            u32::from(REFERRED_COUNT_LONG_FORM_MARKER) << 29 | count,
        );
        bytes.extend_from_slice(retention);
    }

    fn build_segment_header(
        number: u32,
        flags: u8,
        referred_count: ReferredCount,
        referred_numbers: &[u32],
        page_association: u32,
        data_length: u32,
    ) -> Vec<u8> {
        let mut bytes = Vec::new();
        push_u32(&mut bytes, number);
        push_u8(&mut bytes, flags);

        match referred_count {
            ReferredCount::Short(count) => push_short_form_referred_count(&mut bytes, count),
            ReferredCount::Long {
                count,
                retention_bytes,
            } => push_long_form_referred_count(&mut bytes, count, retention_bytes),
        }

        let segment_number_size = if number > TWO_BYTE_SEGMENT_NUMBER_MAX {
            4usize
        } else if number > ONE_BYTE_SEGMENT_NUMBER_MAX {
            2usize
        } else {
            1usize
        };

        for &referred in referred_numbers {
            match segment_number_size {
                1 => push_u8(
                    &mut bytes,
                    u8::try_from(referred).expect("referred number fits in u8"),
                ),
                2 => push_u16(
                    &mut bytes,
                    u16::try_from(referred).expect("referred number fits in u16"),
                ),
                4 => push_u32(&mut bytes, referred),
                _ => unreachable!("valid segment number size"),
            }
        }

        if SegmentHeaderFlagBits::from_bits_retain(flags).page_association_is_32_bit() {
            push_u32(&mut bytes, page_association);
        } else {
            push_u8(
                &mut bytes,
                u8::try_from(page_association).expect("page association fits in u8"),
            );
        }

        push_u32(&mut bytes, data_length);
        bytes
    }

    enum ReferredCount {
        Short(u8),
        Long {
            count: u32,
            retention_bytes: &'static [u8],
        },
    }

    #[test]
    fn long_form_referred_count_skips_retention_bytes() {
        let bytes = build_segment_header(
            257,
            0x00,
            ReferredCount::Long {
                count: 2,
                retention_bytes: &[0xaa],
            },
            &[0x0102, 0x0304],
            0x05,
            0x0000_0006,
        );
        let mut stream = BitReader::new(&bytes);

        let segment = ParsedSegment::try_from(&mut stream).expect("segment header");

        assert_eq!(segment.number, 257);
        assert_eq!(segment.referred_to_segment_numbers, vec![0x0102, 0x0304]);
        assert_eq!(segment.page_association, 0x05);
        assert_eq!(segment.data_length, Some(6));
    }

    #[test]
    fn segment_number_width_boundaries_match_spec() {
        let cases = [
            (ONE_BYTE_SEGMENT_NUMBER_MAX, 0x7f_u32),
            (ONE_BYTE_SEGMENT_NUMBER_MAX + 1, 0x1234_u32),
            (TWO_BYTE_SEGMENT_NUMBER_MAX, 0x1234_u32),
            (TWO_BYTE_SEGMENT_NUMBER_MAX + 1, 0x1234_5678_u32),
        ];

        for (number, referred_number) in cases {
            let bytes = build_segment_header(
                number,
                0x00,
                ReferredCount::Short(1),
                &[referred_number],
                0x01,
                0x0000_0002,
            );
            let mut stream = BitReader::new(&bytes);
            let segment = ParsedSegment::try_from(&mut stream).expect("segment header");

            assert_eq!(segment.referred_to_segment_numbers, vec![referred_number]);
            assert_eq!(segment.number, number);
        }
    }
}
