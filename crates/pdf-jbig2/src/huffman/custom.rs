use pdf_utils::BitReader;

use crate::error::Jbig2Error;

use super::{
    code::{HuffmanCode, assign_canonical_codes},
    decoder::{HuffmanValue, read_extra_bits},
    tree::DecodeTree,
};

const CODE_TABLE_HEADER: &str = "Huffman code table header";
const CODE_TABLE_BITS: &str = "Huffman code table bits";
const CODE_TABLE_RANGE: &str = "Huffman code table range";
const CODE_TABLE_PREFIX_LENGTH: &str = "Huffman code table prefix length";
const CODE_TABLE_RANGE_LENGTH: &str = "Huffman code table range length";
const DECODED_VALUE_OVERFLOW: &str = "Huffman decoded value overflow";
const INTEGER_CONVERSION_OVERFLOW: &str = "integer conversion overflow";
const MAX_CODE_LEN: u8 = 32;
const MAX_RANGE_LEN: u8 = 32;

/// Decoder for a JBIG2 Tables segment carrying a custom Huffman table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CustomHuffmanDecoder {
    entries: Vec<CustomHuffmanEntry>,
    codes: Vec<HuffmanCode>,
    tree: DecodeTree,
}

impl CustomHuffmanDecoder {
    /// Parse a custom Huffman code table segment body.
    pub(crate) fn parse(data: &[u8]) -> Result<Self, Jbig2Error> {
        let mut reader = BitReader::new(data);
        let flags = reader.try_read_u8::<u8>()?;
        let lowest_value = reader.try_read_i32_be()?;
        let highest_value = reader.try_read_i32_be()?;
        if lowest_value > highest_value {
            return Err(Jbig2Error::InvalidTable(CODE_TABLE_RANGE));
        }

        let prefix_size_bits = ((flags >> 1) & 0x07).saturating_add(1);
        let range_size_bits = ((flags >> 4) & 0x07).saturating_add(1);
        let has_oob = flags & 1 != 0;
        if reader.byte_pos() != 9 {
            return Err(Jbig2Error::InvalidTable(CODE_TABLE_HEADER));
        }

        let mut entries = Vec::new();
        let mut lengths = Vec::new();
        let mut current_range_low = lowest_value;
        while current_range_low < highest_value {
            let prefix_len = read_table_bits(&mut reader, prefix_size_bits)?;
            let range_len = read_table_bits(&mut reader, range_size_bits)?;
            validate_prefix_len(prefix_len)?;
            validate_range_len(range_len)?;
            lengths.push(prefix_len);
            entries.push(CustomHuffmanEntry {
                range_low: current_range_low,
                range_len,
                kind: CustomHuffmanEntryKind::Normal,
            });
            let range_size = range_size(range_len)?;
            current_range_low = current_range_low
                .checked_add(range_size)
                .ok_or(Jbig2Error::Overflow(CODE_TABLE_RANGE))?;
        }

        let lower_prefix_len = read_table_bits(&mut reader, prefix_size_bits)?;
        validate_prefix_len(lower_prefix_len)?;
        lengths.push(lower_prefix_len);
        entries.push(CustomHuffmanEntry {
            range_low: lowest_value
                .checked_sub(1)
                .ok_or(Jbig2Error::Overflow(CODE_TABLE_RANGE))?,
            range_len: MAX_RANGE_LEN,
            kind: CustomHuffmanEntryKind::Lower,
        });

        let upper_prefix_len = read_table_bits(&mut reader, prefix_size_bits)?;
        validate_prefix_len(upper_prefix_len)?;
        lengths.push(upper_prefix_len);
        entries.push(CustomHuffmanEntry {
            range_low: highest_value,
            range_len: MAX_RANGE_LEN,
            kind: CustomHuffmanEntryKind::Normal,
        });

        if has_oob {
            let oob_prefix_len = read_table_bits(&mut reader, prefix_size_bits)?;
            validate_prefix_len(oob_prefix_len)?;
            lengths.push(oob_prefix_len);
            entries.push(CustomHuffmanEntry {
                range_low: 0,
                range_len: 0,
                kind: CustomHuffmanEntryKind::OutOfBand,
            });
        }

        let codes = assign_canonical_codes(&lengths)?;
        let tree = DecodeTree::new(&codes)?;
        Ok(Self {
            entries,
            codes,
            tree,
        })
    }

    /// Decode one custom Huffman table value.
    pub(crate) fn decode(&self, reader: &mut BitReader<'_>) -> Result<HuffmanValue, Jbig2Error> {
        let index = self.tree.decode(reader, CODE_TABLE_BITS)?;
        let entry = self
            .entries
            .get(index)
            .ok_or(Jbig2Error::InvalidTable(CODE_TABLE_HEADER))?;
        if entry.kind == CustomHuffmanEntryKind::OutOfBand {
            return Ok(HuffmanValue::OutOfBand);
        }

        let extra = read_extra_bits(reader, entry.range_len)?;
        let decoded = if entry.kind == CustomHuffmanEntryKind::Lower {
            entry.range_low.checked_sub(extra)
        } else {
            entry.range_low.checked_add(extra)
        }
        .ok_or(Jbig2Error::Overflow(DECODED_VALUE_OVERFLOW))?;
        Ok(HuffmanValue::Value(decoded))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CustomHuffmanEntry {
    range_low: i32,
    range_len: u8,
    kind: CustomHuffmanEntryKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CustomHuffmanEntryKind {
    Normal,
    Lower,
    OutOfBand,
}

fn read_table_bits(reader: &mut BitReader<'_>, bits: u8) -> Result<u8, Jbig2Error> {
    let value = reader
        .read_bits(bits)
        .ok_or(Jbig2Error::Truncated(CODE_TABLE_BITS))?;
    u8::try_from(value).map_err(|_| Jbig2Error::Overflow(INTEGER_CONVERSION_OVERFLOW))
}

fn validate_prefix_len(prefix_len: u8) -> Result<(), Jbig2Error> {
    if prefix_len > MAX_CODE_LEN {
        return Err(Jbig2Error::InvalidTable(CODE_TABLE_PREFIX_LENGTH));
    }
    Ok(())
}

fn validate_range_len(range_len: u8) -> Result<(), Jbig2Error> {
    if range_len > MAX_RANGE_LEN {
        return Err(Jbig2Error::InvalidTable(CODE_TABLE_RANGE_LENGTH));
    }
    Ok(())
}

fn range_size(range_len: u8) -> Result<i32, Jbig2Error> {
    if range_len >= 31 {
        return Err(Jbig2Error::InvalidTable(CODE_TABLE_RANGE_LENGTH));
    }
    1i32.checked_shl(u32::from(range_len))
        .ok_or(Jbig2Error::Overflow(CODE_TABLE_RANGE))
}

#[cfg(test)]
mod tests {
    use pdf_utils::BitReader;

    use super::CustomHuffmanDecoder;
    use crate::{error::Jbig2Error, huffman::HuffmanValue};

    fn push_header(bytes: &mut Vec<u8>, flags: u8, lowest_value: i32, highest_value: i32) {
        bytes.push(flags);
        bytes.extend_from_slice(&lowest_value.to_be_bytes());
        bytes.extend_from_slice(&highest_value.to_be_bytes());
    }

    fn push_bits(bits: &mut Vec<bool>, value: u32, width: u8) {
        for shift in (0..u32::from(width)).rev() {
            bits.push(((value >> shift) & 1) != 0);
        }
    }

    fn push_bits_as_bytes(bytes: &mut Vec<u8>, bits: &[bool]) {
        let mut current = 0u8;
        for (index, bit) in bits.iter().copied().enumerate() {
            if bit {
                current |= 1u8 << (7usize.saturating_sub(index % 8));
            }
            if index % 8 == 7 {
                bytes.push(current);
                current = 0;
            }
        }
        if bits.len() % 8 != 0 {
            bytes.push(current);
        }
    }

    fn simple_table() -> CustomHuffmanDecoder {
        let mut data = Vec::new();
        push_header(&mut data, 0b0000_0010, 0, 2);
        let mut bits = Vec::new();
        push_bits(&mut bits, 1, 2);
        push_bits(&mut bits, 0, 1);
        push_bits(&mut bits, 2, 2);
        push_bits(&mut bits, 0, 1);
        push_bits(&mut bits, 0, 2);
        push_bits(&mut bits, 2, 2);
        push_bits_as_bytes(&mut data, &bits);
        CustomHuffmanDecoder::parse(&data).expect("custom table")
    }

    #[test]
    fn decodes_custom_table_normal_ranges() {
        let table = simple_table();
        let data = [0b0100_0000u8];
        let mut reader = BitReader::new(&data);

        assert_eq!(table.decode(&mut reader), Ok(HuffmanValue::Value(0)));
        assert_eq!(table.decode(&mut reader), Ok(HuffmanValue::Value(1)));
    }

    #[test]
    fn decodes_custom_table_upper_open_range() {
        let table = simple_table();
        let mut bits = Vec::new();
        push_bits(&mut bits, 0b11, 2);
        push_bits(&mut bits, 5, 32);
        let mut data = Vec::new();
        push_bits_as_bytes(&mut data, &bits);
        let mut reader = BitReader::new(&data);

        assert_eq!(table.decode(&mut reader), Ok(HuffmanValue::Value(7)));
    }

    #[test]
    fn decodes_custom_table_oob_row() {
        let mut data = Vec::new();
        push_header(&mut data, 0b0000_0011, 0, 1);
        let mut bits = Vec::new();
        push_bits(&mut bits, 1, 2);
        push_bits(&mut bits, 0, 1);
        push_bits(&mut bits, 0, 2);
        push_bits(&mut bits, 2, 2);
        push_bits(&mut bits, 2, 2);
        push_bits_as_bytes(&mut data, &bits);
        let table = CustomHuffmanDecoder::parse(&data).expect("custom table");
        let encoded = [0b1100_0000u8];
        let mut reader = BitReader::new(&encoded);

        assert_eq!(table.decode(&mut reader), Ok(HuffmanValue::OutOfBand));
    }

    #[test]
    fn rejects_truncated_custom_table() {
        let err = CustomHuffmanDecoder::parse(&[0]).expect_err("truncated");

        assert_eq!(err, Jbig2Error::Truncated("byte-aligned read"));
    }
}
