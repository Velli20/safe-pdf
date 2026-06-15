use crate::error::Jbig2Error;
use pdf_utils::BitReader;

use super::{
    code::{HuffmanCode, assign_canonical_codes_from_lengths},
    standard::{HuffmanRangeEntry, STANDARD_TABLES, StandardTableId},
    tree::DecodeTree,
};

const HUFFMAN_STREAM_NAME: &str = "Huffman stream";
const HUFFMAN_ENTRY_ERROR: &str = "Huffman entry";
const HUFFMAN_TABLE_ERROR: &str = "Huffman table";
const EXTRA_BITS_OVERFLOW: &str = "Huffman extra bits overflow";
const DECODED_VALUE_OVERFLOW: &str = "Huffman decoded value overflow";
const OOB_ENTRY_FROM_END: usize = 1;
const LOWER_RANGE_ENTRY_FROM_END_WITH_OOB: usize = 3;
const LOWER_RANGE_ENTRY_FROM_END_WITHOUT_OOB: usize = 2;

/// Result of decoding one JBIG2 Huffman table value.
///
/// ITU-T T.88 / ISO/IEC 14492 Annex B allows a decoded row to be either an
/// integer value or the `HTOOB` out-of-band marker used by higher-level JBIG2
/// procedures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HuffmanValue {
    /// Integer value decoded from a range row and any following extra bits.
    Value(i32),
    /// Huffman out-of-band marker from a table whose `HTOOB` flag is set.
    OutOfBand,
}

/// Decoder for one standard JBIG2 Huffman table from Annex B.
///
/// Construction assigns canonical prefix codes for the selected standard table
/// and builds a decode tree. Decoding follows the ITU-T T.88 / ISO/IEC 14492
/// Annex B Huffman Table Decoding Procedure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StandardHuffmanDecoder {
    entries: &'static [HuffmanRangeEntry],
    codes: Vec<HuffmanCode>,
    tree: DecodeTree,
    htoob: bool,
}

impl StandardHuffmanDecoder {
    /// Build a decoder for one named standard Huffman table.
    ///
    /// `table_id` is one of the `STANDARD_TABLE_B*` constants matching ITU-T
    /// T.88 / ISO/IEC 14492 Annex B table numbers.
    pub(crate) fn new(table_id: StandardTableId) -> Result<Self, Jbig2Error> {
        let definition = STANDARD_TABLES
            .get(table_id.lookup_index())
            .ok_or(Jbig2Error::InvalidTable(HUFFMAN_TABLE_ERROR))?;
        let lengths = definition.entries.iter().map(|entry| entry.prefix_len);
        let codes = assign_canonical_codes_from_lengths(definition.entries.len(), lengths)?;
        let tree = DecodeTree::new(&codes)?;
        Ok(Self {
            entries: definition.entries,
            codes,
            tree,
            htoob: definition.htoob,
        })
    }

    /// Decode one Huffman-coded integer or out-of-band marker.
    ///
    /// This implements ITU-T T.88 / ISO/IEC 14492 Annex B after the prefix row
    /// has been found: read `RANGELEN` extra bits and add or subtract them
    /// from `RANGELOW` depending on whether the row is the lower open-ended
    /// range.
    pub(crate) fn decode(&self, reader: &mut BitReader<'_>) -> Result<HuffmanValue, Jbig2Error> {
        let index = self.tree.decode(reader, HUFFMAN_STREAM_NAME)?;
        if self.is_out_of_band(index) {
            return Ok(HuffmanValue::OutOfBand);
        }

        let entry = self
            .entries
            .get(index)
            .ok_or(Jbig2Error::InvalidTable(HUFFMAN_ENTRY_ERROR))?;
        let extra = read_extra_bits(reader, entry.range_len)?;
        let decoded = if self.is_lower_open_range(index) {
            entry.range_low.checked_sub(extra)
        } else {
            entry.range_low.checked_add(extra)
        }
        .ok_or(Jbig2Error::Overflow(DECODED_VALUE_OVERFLOW))?;

        Ok(HuffmanValue::Value(decoded))
    }

    /// Return the canonical codes assigned to this Annex B table.
    ///
    /// This is exposed only to tests so fixture builders can verify code
    /// assignment without duplicating production state.
    #[cfg(test)]
    pub(crate) fn codes(&self) -> &[HuffmanCode] {
        &self.codes
    }

    /// Return the table entry and code for `index`.
    ///
    /// This test-only helper supports fixture encoding while keeping the
    /// production decoder focused on Annex B decoding.
    #[cfg(test)]
    pub(crate) fn table_row(
        &self,
        index: usize,
    ) -> Option<(&'static HuffmanRangeEntry, HuffmanCode)> {
        Some((self.entries.get(index)?, *self.codes.get(index)?))
    }

    /// Return whether `index` points at the Annex B out-of-band row.
    ///
    /// When `HTOOB` is set, Annex B stores the OOB row at the end of the
    /// standard table.
    fn is_out_of_band(&self, index: usize) -> bool {
        self.htoob && index.saturating_add(OOB_ENTRY_FROM_END) == self.codes.len()
    }

    /// Return whether `index` is the lower open-ended range row.
    ///
    /// Annex B places open-ended lower and upper range rows near the end of
    /// the standard table; an OOB row shifts the lower row one position
    /// farther from the end.
    fn is_lower_open_range(&self, index: usize) -> bool {
        index == self.lower_open_range_index()
    }

    /// Return the index of the lower open-ended range row in this table.
    ///
    /// The returned row is the one whose decoded extra bits are subtracted
    /// from `RANGELOW` in the Annex B decoding procedure.
    fn lower_open_range_index(&self) -> usize {
        let offset = if self.htoob {
            LOWER_RANGE_ENTRY_FROM_END_WITH_OOB
        } else {
            LOWER_RANGE_ENTRY_FROM_END_WITHOUT_OOB
        };
        self.codes.len().saturating_sub(offset)
    }
}

/// Read the extra bits that follow a matched Huffman prefix.
///
/// ITU-T T.88 / ISO/IEC 14492 Annex B names this count `RANGELEN`; bits are
/// read most-significant first and interpreted as an unsigned integer before
/// being added to or subtracted from `RANGELOW`.
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

#[cfg(test)]
mod tests {
    use pdf_utils::BitReader;

    use crate::huffman::{
        HuffmanValue, StandardHuffmanDecoder,
        standard::{STANDARD_TABLE_B7, STANDARD_TABLE_B8},
        test_support::{bits_to_bytes, encode_standard_huffman_value},
    };

    #[test]
    fn assigns_standard_b7_codes() {
        let table = StandardHuffmanDecoder::new(STANDARD_TABLE_B7).expect("table");
        assert_eq!(table.codes().len(), 15);
        assert!(table.codes().iter().all(|code| code.codelen > 0));
    }

    #[test]
    fn decodes_standard_table_oob_marker() {
        let table = StandardHuffmanDecoder::new(STANDARD_TABLE_B8).expect("table");
        let mut bits = Vec::new();
        encode_standard_huffman_value(&mut bits, &table, HuffmanValue::OutOfBand)
            .expect("oob bits");
        let data = bits_to_bytes(&bits);
        let mut reader = BitReader::new(&data);

        let result = table.decode(&mut reader).expect("decode");

        assert_eq!(result, HuffmanValue::OutOfBand);
    }

    #[test]
    fn decodes_standard_table_value_with_extra_bits() {
        let table = StandardHuffmanDecoder::new(STANDARD_TABLE_B8).expect("table");
        let mut bits = Vec::new();
        encode_standard_huffman_value(&mut bits, &table, HuffmanValue::Value(0))
            .expect("value bits");
        let data = bits_to_bytes(&bits);
        let mut reader = BitReader::new(&data);

        let result = table.decode(&mut reader).expect("decode");

        assert_eq!(result, HuffmanValue::Value(0));
    }
}
