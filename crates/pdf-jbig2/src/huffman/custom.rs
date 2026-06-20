use bitflags::bitflags;
use pdf_utils::BitReader;

use crate::error::Jbig2Error;

use super::{
    code::{HuffmanCode, assign_canonical_codes},
    decoder::HuffmanValue,
    range::{
        CODE_TABLE_BITS, CODE_TABLE_HEADER, CODE_TABLE_RANGE, HuffmanRangeKind, decode_range_value,
        range_size, validate_prefix_len, validate_range_len,
    },
    tree::DecodeTree,
};

const MAX_RANGE_LEN: u8 = 32;
const PREFIX_SIZE_SHIFT: u8 = 1;
const RANGE_SIZE_SHIFT: u8 = 4;

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    struct CustomHuffmanTableFlags: u8 {
        const HAS_OOB = 1 << 0;
        const PREFIX_SIZE_MASK = 0b0000_1110;
        const RANGE_SIZE_MASK = 0b0111_0000;
    }
}

impl CustomHuffmanTableFlags {
    fn prefix_size_bits(self) -> u8 {
        self.field_bits(Self::PREFIX_SIZE_MASK, PREFIX_SIZE_SHIFT)
            .saturating_add(1)
    }

    fn range_size_bits(self) -> u8 {
        self.field_bits(Self::RANGE_SIZE_MASK, RANGE_SIZE_SHIFT)
            .saturating_add(1)
    }

    fn has_oob(self) -> bool {
        self.contains(Self::HAS_OOB)
    }

    fn field_bits(self, mask: Self, shift: u8) -> u8 {
        (self.bits() & mask.bits()) >> shift
    }
}

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
        let flags = CustomHuffmanTableFlags::from_bits_retain(reader.try_read_u8::<u8>()?);
        let lowest_value = reader.try_read_i32_be()?;
        let highest_value = reader.try_read_i32_be()?;
        if lowest_value > highest_value {
            return Err(Jbig2Error::InvalidTable(CODE_TABLE_RANGE));
        }

        let prefix_size_bits = flags.prefix_size_bits();
        let range_size_bits = flags.range_size_bits();
        let has_oob = flags.has_oob();
        if reader.byte_pos() != 9 {
            return Err(Jbig2Error::InvalidTable(CODE_TABLE_HEADER));
        }

        let mut entries = Vec::new();
        let mut lengths = Vec::new();
        let mut current_range_low = lowest_value;
        while current_range_low < highest_value {
            let prefix_len = reader.try_read_bits_u8(prefix_size_bits, CODE_TABLE_BITS)?;
            let range_len = reader.try_read_bits_u8(range_size_bits, CODE_TABLE_BITS)?;
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

        let lower_prefix_len = reader.try_read_bits_u8(prefix_size_bits, CODE_TABLE_BITS)?;
        validate_prefix_len(lower_prefix_len)?;
        lengths.push(lower_prefix_len);
        entries.push(CustomHuffmanEntry {
            range_low: lowest_value
                .checked_sub(1)
                .ok_or(Jbig2Error::Overflow(CODE_TABLE_RANGE))?,
            range_len: MAX_RANGE_LEN,
            kind: CustomHuffmanEntryKind::Lower,
        });

        let upper_prefix_len = reader.try_read_bits_u8(prefix_size_bits, CODE_TABLE_BITS)?;
        validate_prefix_len(upper_prefix_len)?;
        lengths.push(upper_prefix_len);
        entries.push(CustomHuffmanEntry {
            range_low: highest_value,
            range_len: MAX_RANGE_LEN,
            kind: CustomHuffmanEntryKind::Normal,
        });

        if has_oob {
            let oob_prefix_len = reader.try_read_bits_u8(prefix_size_bits, CODE_TABLE_BITS)?;
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
        decode_range_value(entry.range_low, entry.range_len, entry.kind.into(), reader)
    }

    /// Decode one custom Huffman table integer, rejecting out-of-band markers.
    pub(crate) fn decode_value(&self, reader: &mut BitReader<'_>) -> Result<i32, Jbig2Error> {
        match self.decode(reader)? {
            HuffmanValue::Value(value) => Ok(value),
            HuffmanValue::OutOfBand => Err(Jbig2Error::UnexpectedHuffmanOob),
        }
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

impl From<CustomHuffmanEntryKind> for HuffmanRangeKind {
    fn from(value: CustomHuffmanEntryKind) -> Self {
        match value {
            CustomHuffmanEntryKind::Normal => Self::Normal,
            CustomHuffmanEntryKind::Lower => Self::Lower,
            CustomHuffmanEntryKind::OutOfBand => Self::OutOfBand,
        }
    }
}

#[cfg(test)]
mod tests {
    use pdf_utils::BitReader;

    use super::CustomHuffmanDecoder;
    use crate::{
        error::Jbig2Error,
        huffman::{
            HuffmanValue,
            test_support::{bits_to_bytes, push_bits},
        },
    };

    fn push_header(bytes: &mut Vec<u8>, flags: u8, lowest_value: i32, highest_value: i32) {
        bytes.push(flags);
        bytes.extend_from_slice(&lowest_value.to_be_bytes());
        bytes.extend_from_slice(&highest_value.to_be_bytes());
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
        data.extend_from_slice(&bits_to_bytes(&bits));
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
        let data = bits_to_bytes(&bits);
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
        data.extend_from_slice(&bits_to_bytes(&bits));
        let table = CustomHuffmanDecoder::parse(&data).expect("custom table");
        let encoded = [0b1100_0000u8];
        let mut reader = BitReader::new(&encoded);

        assert_eq!(table.decode(&mut reader), Ok(HuffmanValue::OutOfBand));
    }

    #[test]
    fn decode_value_rejects_custom_table_oob_row() {
        let mut data = Vec::new();
        push_header(&mut data, 0b0000_0011, 0, 1);
        let mut bits = Vec::new();
        push_bits(&mut bits, 1, 2);
        push_bits(&mut bits, 0, 1);
        push_bits(&mut bits, 0, 2);
        push_bits(&mut bits, 2, 2);
        push_bits(&mut bits, 2, 2);
        data.extend_from_slice(&bits_to_bytes(&bits));
        let table = CustomHuffmanDecoder::parse(&data).expect("custom table");
        let encoded = [0b1100_0000u8];
        let mut reader = BitReader::new(&encoded);

        assert_eq!(
            table.decode_value(&mut reader),
            Err(Jbig2Error::UnexpectedHuffmanOob)
        );
    }

    #[test]
    fn rejects_truncated_custom_table() {
        let err = CustomHuffmanDecoder::parse(&[0]).expect_err("truncated");

        assert_eq!(err, Jbig2Error::Truncated("byte-aligned read"));
    }
}
