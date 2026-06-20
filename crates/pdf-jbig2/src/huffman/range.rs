use pdf_utils::BitReader;

use crate::error::Jbig2Error;

use super::decoder::HuffmanValue;

pub(crate) const CODE_TABLE_HEADER: &str = "Huffman code table header";
pub(crate) const CODE_TABLE_BITS: &str = "Huffman code table bits";
pub(crate) const CODE_TABLE_PREFIX_LENGTH: &str = "Huffman code table prefix length";
pub(crate) const CODE_TABLE_RANGE: &str = "Huffman code table range";
pub(crate) const CODE_TABLE_RANGE_LENGTH: &str = "Huffman code table range length";
const DECODED_VALUE_OVERFLOW: &str = "Huffman decoded value overflow";
const EXTRA_BITS_OVERFLOW: &str = "Huffman extra bits overflow";
const HUFFMAN_STREAM_NAME: &str = "Huffman stream";
const MAX_CODE_LEN: u8 = 32;
const MAX_RANGE_LEN: u8 = 32;

/// One Huffman range row used by standard and custom table decoders.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HuffmanRangeEntry {
    pub(crate) prefix_len: u8,
    pub(crate) range_len: u8,
    pub(crate) range_low: i32,
}

/// Row type for a Huffman range entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HuffmanRangeKind {
    Normal,
    Lower,
    OutOfBand,
}

/// Read `bit_len` extra bits from a Huffman stream.
pub(crate) fn read_extra_bits(reader: &mut BitReader<'_>, bit_len: u8) -> Result<i32, Jbig2Error> {
    let mut value = 0i32;
    for _ in 0..bit_len {
        let bit = reader
            .next_bit()
            .ok_or(Jbig2Error::Truncated(HUFFMAN_STREAM_NAME))?;
        value = value
            .checked_shl(1)
            .ok_or(Jbig2Error::Overflow(EXTRA_BITS_OVERFLOW))?;
        if bit {
            value = value
                .checked_add(1)
                .ok_or(Jbig2Error::Overflow(EXTRA_BITS_OVERFLOW))?;
        }
    }
    Ok(value)
}

/// Validate a decoded prefix length.
pub(crate) fn validate_prefix_len(prefix_len: u8) -> Result<(), Jbig2Error> {
    if prefix_len > MAX_CODE_LEN {
        return Err(Jbig2Error::InvalidTable(CODE_TABLE_PREFIX_LENGTH));
    }
    Ok(())
}

/// Validate a decoded range length.
pub(crate) fn validate_range_len(range_len: u8) -> Result<(), Jbig2Error> {
    if range_len > MAX_RANGE_LEN {
        return Err(Jbig2Error::InvalidTable(CODE_TABLE_RANGE_LENGTH));
    }
    Ok(())
}

/// Compute the number of values covered by a Huffman range row.
pub(crate) fn range_size(range_len: u8) -> Result<i32, Jbig2Error> {
    if range_len >= 31 {
        return Err(Jbig2Error::InvalidTable(CODE_TABLE_RANGE_LENGTH));
    }
    1i32.checked_shl(u32::from(range_len))
        .ok_or(Jbig2Error::Overflow(CODE_TABLE_RANGE))
}

/// Decode one matched Huffman range row.
pub(crate) fn decode_range_value(
    range_low: i32,
    range_len: u8,
    kind: HuffmanRangeKind,
    reader: &mut BitReader<'_>,
) -> Result<HuffmanValue, Jbig2Error> {
    if matches!(kind, HuffmanRangeKind::OutOfBand) {
        return Ok(HuffmanValue::OutOfBand);
    }

    let extra = read_extra_bits(reader, range_len)?;
    let decoded = if matches!(kind, HuffmanRangeKind::Lower) {
        range_low.checked_sub(extra)
    } else {
        range_low.checked_add(extra)
    }
    .ok_or(Jbig2Error::Overflow(DECODED_VALUE_OVERFLOW))?;
    Ok(HuffmanValue::Value(decoded))
}

#[cfg(test)]
mod tests {
    use super::{HuffmanRangeKind, decode_range_value};
    use crate::huffman::HuffmanValue;
    use pdf_utils::BitReader;

    #[test]
    fn decodes_normal_range_value() {
        let data = [0b1000_0000u8];
        let mut reader = BitReader::new(&data);

        let value = decode_range_value(1, 1, HuffmanRangeKind::Normal, &mut reader).expect("value");

        assert_eq!(value, HuffmanValue::Value(2));
    }
}
