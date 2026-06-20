use crate::error::Jbig2Error;
use pdf_utils::BitReader;

use super::{
    code::{HuffmanCode, assign_canonical_codes},
    tree::DecodeTree,
};

/// Number of run-code entries used to encode symbol-ID code lengths.
///
/// ITU-T T.88 / ISO/IEC 14492 section 6.4.10 defines a 35-entry Huffman table
/// for literal code lengths and repeat commands.
const RUN_CODE_COUNT: usize = 35;
/// Number of bits used to store each run-code table prefix length.
///
/// ITU-T T.88 / ISO/IEC 14492 section 6.4.10 stores each run-code table length
/// in a four-bit field.
const RUN_CODE_LENGTH_BITS: u8 = 4;
/// Number of literal symbol-ID code-length commands.
///
/// Run codes `0..32` are literal symbol-ID code lengths in ITU-T T.88 /
/// ISO/IEC 14492 section 6.4.10.
const LITERAL_RUN_CODE_COUNT: usize = 32;
/// Run-code command that repeats the previous symbol-ID code length.
const REPEAT_PREVIOUS_RUN_CODE: usize = 32;
/// Run-code command that repeats zero code lengths with a short extra field.
const REPEAT_ZERO_SHORT_RUN_CODE: usize = 33;
/// Run-code command that repeats zero code lengths with a long extra field.
const REPEAT_ZERO_LONG_RUN_CODE: usize = 34;
/// Extra bits following the repeat-previous command in section 6.4.10.
const REPEAT_PREVIOUS_EXTRA_BITS: u8 = 2;
/// Base repeat count for the repeat-previous command in section 6.4.10.
const REPEAT_PREVIOUS_BASE: usize = 3;
/// Extra bits following the short zero-repeat command in section 6.4.10.
const REPEAT_ZERO_SHORT_EXTRA_BITS: u8 = 3;
/// Base repeat count for the short zero-repeat command in section 6.4.10.
const REPEAT_ZERO_SHORT_BASE: usize = 3;
/// Extra bits following the long zero-repeat command in section 6.4.10.
const REPEAT_ZERO_LONG_EXTRA_BITS: u8 = 7;
/// Base repeat count for the long zero-repeat command in section 6.4.10.
const REPEAT_ZERO_LONG_BASE: usize = 11;
/// Code length used for repeated zero-length symbol-ID codes.
const ZERO_CODE_LENGTH: u8 = 0;
const SYMBOL_ID_TABLE_STREAM: &str = "symbol-id Huffman table";
const SYMBOL_ID_STREAM: &str = "symbol-id stream";

/// Repeat metadata for a symbol-ID code-length run command.
///
/// ITU-T T.88 / ISO/IEC 14492 section 6.4.10 defines each repeat command as a
/// base count plus an unsigned extra-bit field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RepeatCommand {
    extra_bits: u8,
    base: usize,
}

impl RepeatCommand {
    /// Read this command's repeat count from `reader`.
    ///
    /// The returned count is `base + extra`, matching ITU-T T.88 /
    /// ISO/IEC 14492 section 6.4.10 for symbol-ID code-length repeat
    /// commands.
    fn read_count(self, reader: &mut BitReader<'_>) -> Result<usize, Jbig2Error> {
        let extra = reader
            .read_bits(self.extra_bits)
            .ok_or(Jbig2Error::Truncated(SYMBOL_ID_TABLE_STREAM))?;
        Ok(usize::from(extra).saturating_add(self.base))
    }
}

/// Decoded command from the symbol-ID run-code Huffman table.
///
/// ITU-T T.88 / ISO/IEC 14492 section 6.4.10 maps run-code symbols `0..31` to
/// literal symbol-ID code lengths and symbols `32..34` to repeat commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SymbolIdRunCommand {
    Literal(u8),
    RepeatPrevious(RepeatCommand),
    RepeatZero(RepeatCommand),
}

impl SymbolIdRunCommand {
    /// Convert a decoded run-code symbol into its section 6.4.10 command.
    ///
    /// Symbols outside the 35-entry run-code table are invalid for the
    /// symbol-ID Huffman table procedure.
    fn from_symbol(symbol: usize) -> Result<Self, Jbig2Error> {
        match symbol {
            literal @ 0..LITERAL_RUN_CODE_COUNT => {
                let code_length = literal
                    .try_into()
                    .map_err(|_| Jbig2Error::InvalidTable(SYMBOL_ID_TABLE_STREAM))?;
                Ok(Self::Literal(code_length))
            }
            REPEAT_PREVIOUS_RUN_CODE => Ok(Self::RepeatPrevious(RepeatCommand {
                extra_bits: REPEAT_PREVIOUS_EXTRA_BITS,
                base: REPEAT_PREVIOUS_BASE,
            })),
            REPEAT_ZERO_SHORT_RUN_CODE => Ok(Self::RepeatZero(RepeatCommand {
                extra_bits: REPEAT_ZERO_SHORT_EXTRA_BITS,
                base: REPEAT_ZERO_SHORT_BASE,
            })),
            REPEAT_ZERO_LONG_RUN_CODE => Ok(Self::RepeatZero(RepeatCommand {
                extra_bits: REPEAT_ZERO_LONG_EXTRA_BITS,
                base: REPEAT_ZERO_LONG_BASE,
            })),
            _ => Err(Jbig2Error::InvalidTable(SYMBOL_ID_TABLE_STREAM)),
        }
    }
}

/// Decode tree for the 35-entry symbol-ID run-code table.
///
/// ITU-T T.88 / ISO/IEC 14492 section 6.4.10 first decodes this table from
/// four-bit code-length fields, then uses it to read symbol-ID code lengths.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SymbolIdRunCodeTable {
    tree: DecodeTree,
}

impl SymbolIdRunCodeTable {
    /// Read and build the section 6.4.10 run-code Huffman table.
    ///
    /// The input is exactly 35 four-bit prefix lengths, followed by the
    /// Huffman-coded symbol-ID code-length commands consumed by [`Self::decode`].
    fn read(reader: &mut BitReader<'_>) -> Result<Self, Jbig2Error> {
        let lengths = read_run_code_lengths(reader)?;
        let codes = assign_canonical_codes(&lengths)?;
        let tree = DecodeTree::new(&codes)?;
        Ok(Self { tree })
    }

    /// Decode one section 6.4.10 run-code command from `reader`.
    ///
    /// The command controls either one literal symbol-ID code length or a run
    /// of repeated code lengths.
    fn decode(&self, reader: &mut BitReader<'_>) -> Result<SymbolIdRunCommand, Jbig2Error> {
        let symbol = self.tree.decode(reader, SYMBOL_ID_TABLE_STREAM)?;
        SymbolIdRunCommand::from_symbol(symbol)
    }
}

/// Huffman table for text-region symbol identifiers.
///
/// ITU-T T.88 / ISO/IEC 14492 section 6.4.10 decodes symbol IDs using a
/// Huffman table whose code lengths are themselves Huffman-coded in the text
/// region segment body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SymbolIdHuffmanTable {
    codes: Vec<HuffmanCode>,
    tree: DecodeTree,
}

impl SymbolIdHuffmanTable {
    /// Build a symbol-ID table from decoded symbol code lengths.
    ///
    /// The canonical assignment follows the same ITU-T T.88 / ISO/IEC 14492
    /// Annex B canonical Huffman-code construction used by standard tables.
    fn new(lengths: &[u8]) -> Result<Self, Jbig2Error> {
        let codes = assign_canonical_codes(lengths)?;
        let tree = DecodeTree::new(&codes)?;
        Ok(Self { codes, tree })
    }

    /// Decode one symbol ID from a text-region Huffman body.
    ///
    /// This is the symbol ID lookup step described by ITU-T T.88 /
    /// ISO/IEC 14492 section 6.4.10.
    fn decode(&self, reader: &mut BitReader<'_>) -> Result<usize, Jbig2Error> {
        self.tree.decode(reader, SYMBOL_ID_STREAM)
    }

    /// Return canonical symbol-ID codes for focused unit tests.
    ///
    /// Tests use this to validate the code-length parsing procedure from
    /// ITU-T T.88 / ISO/IEC 14492 section 6.4.10.
    #[cfg(test)]
    pub(crate) fn codes(&self) -> &[HuffmanCode] {
        &self.codes
    }
}

/// Decode the text-region symbol-ID Huffman table from `reader`.
///
/// ITU-T T.88 / ISO/IEC 14492 section 6.4.10 stores the symbol-ID code
/// lengths using a 35-entry run-code table followed by literal and repeat
/// commands.
pub(crate) fn decode_symbol_id_huffman_table(
    reader: &mut BitReader<'_>,
    symbol_count: usize,
) -> Result<SymbolIdHuffmanTable, Jbig2Error> {
    let run_code_table = SymbolIdRunCodeTable::read(reader)?;
    let symbol_code_lengths = decode_symbol_code_lengths(reader, &run_code_table, symbol_count)?;
    SymbolIdHuffmanTable::new(&symbol_code_lengths)
}

/// Decode one symbol ID using a parsed symbol-ID Huffman table.
///
/// This wrapper keeps text-region callers aligned with ITU-T T.88 /
/// ISO/IEC 14492 section 6.4.10 without exposing the table internals.
pub(crate) fn decode_symbol_id(
    reader: &mut BitReader<'_>,
    table: &SymbolIdHuffmanTable,
) -> Result<usize, Jbig2Error> {
    table.decode(reader)
}

/// Read the 35 run-code table lengths from `reader`.
///
/// ITU-T T.88 / ISO/IEC 14492 section 6.4.10 stores these lengths as fixed
/// four-bit values before the symbol-ID code-length command stream.
fn read_run_code_lengths(reader: &mut BitReader<'_>) -> Result<[u8; RUN_CODE_COUNT], Jbig2Error> {
    let mut lengths = [ZERO_CODE_LENGTH; RUN_CODE_COUNT];
    for length in &mut lengths {
        let codelen = reader
            .read_bits(RUN_CODE_LENGTH_BITS)
            .ok_or(Jbig2Error::Truncated(SYMBOL_ID_TABLE_STREAM))?;
        *length = codelen
            .try_into()
            .map_err(|_| Jbig2Error::InvalidTable(SYMBOL_ID_TABLE_STREAM))?;
    }
    Ok(lengths)
}

/// Decode all symbol-ID Huffman code lengths.
///
/// ITU-T T.88 / ISO/IEC 14492 section 6.4.10 uses the run-code table to
/// produce exactly one code length per symbol in the text region's symbol set.
fn decode_symbol_code_lengths(
    reader: &mut BitReader<'_>,
    run_code_table: &SymbolIdRunCodeTable,
    symbol_count: usize,
) -> Result<Vec<u8>, Jbig2Error> {
    let mut lengths = vec![ZERO_CODE_LENGTH; symbol_count];
    let mut cursor = 0usize;

    while cursor < symbol_count {
        let command = run_code_table.decode(reader)?;
        cursor = apply_run_command(reader, command, &mut lengths, cursor)?;
    }

    Ok(lengths)
}

/// Apply one decoded symbol-ID run command.
///
/// ITU-T T.88 / ISO/IEC 14492 section 6.4.10 defines literal commands for
/// individual code lengths and repeat commands for previous or zero lengths.
fn apply_run_command(
    reader: &mut BitReader<'_>,
    command: SymbolIdRunCommand,
    lengths: &mut [u8],
    cursor: usize,
) -> Result<usize, Jbig2Error> {
    match command {
        SymbolIdRunCommand::Literal(code_length) => write_literal(lengths, cursor, code_length),
        SymbolIdRunCommand::RepeatPrevious(repeat) => {
            let value = previous_code_length(lengths, cursor)?;
            fill_run(lengths, cursor, repeat.read_count(reader)?, value)
        }
        SymbolIdRunCommand::RepeatZero(repeat) => fill_run(
            lengths,
            cursor,
            repeat.read_count(reader)?,
            ZERO_CODE_LENGTH,
        ),
    }
}

/// Write one literal symbol-ID code length.
///
/// This is the literal `0..31` run-code case from ITU-T T.88 / ISO/IEC 14492
/// section 6.4.10.
fn write_literal(lengths: &mut [u8], cursor: usize, value: u8) -> Result<usize, Jbig2Error> {
    let slot = lengths
        .get_mut(cursor)
        .ok_or(Jbig2Error::InvalidTable(SYMBOL_ID_TABLE_STREAM))?;
    *slot = value;
    cursor.checked_add(1).ok_or(Jbig2Error::Overflow(
        "symbol-id code-length cursor overflow",
    ))
}

/// Return the previous symbol-ID code length for a repeat-previous command.
///
/// ITU-T T.88 / ISO/IEC 14492 section 6.4.10 repeats the prior decoded code
/// length; at the beginning of the list this decoder treats the prior value as
/// zero, matching the existing symbol-ID table behavior.
fn previous_code_length(lengths: &[u8], cursor: usize) -> Result<u8, Jbig2Error> {
    if cursor == 0 {
        return Ok(ZERO_CODE_LENGTH);
    }

    let previous = cursor.checked_sub(1).ok_or(Jbig2Error::Overflow(
        "symbol-id code-length cursor overflow",
    ))?;
    lengths
        .get(previous)
        .copied()
        .ok_or(Jbig2Error::InvalidTable(SYMBOL_ID_TABLE_STREAM))
}

/// Fill a repeated run of symbol-ID code lengths.
///
/// ITU-T T.88 / ISO/IEC 14492 section 6.4.10 uses run commands to repeat the
/// previous code length or zero code lengths without changing the number of
/// entries in the symbol-ID code-length array.
fn fill_run(
    lengths: &mut [u8],
    start: usize,
    repeat: usize,
    value: u8,
) -> Result<usize, Jbig2Error> {
    let unclamped_end = start
        .checked_add(repeat)
        .ok_or(Jbig2Error::Overflow("integer conversion overflow"))?;
    let end = unclamped_end.min(lengths.len());

    let run = lengths
        .get_mut(start..end)
        .ok_or(Jbig2Error::InvalidTable(SYMBOL_ID_TABLE_STREAM))?;
    run.fill(value);
    Ok(end)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::huffman::test_support::{bits_to_bytes, push_bits};

    #[test]
    fn decodes_literal_code_lengths() {
        let data = symbol_id_table_stream(&[(2, 1), (5, 2)], &[(0, 1), (2, 2)]);
        let mut reader = BitReader::new(&data);
        let table = decode_symbol_id_huffman_table(&mut reader, 2).expect("table");

        assert_code_lengths(&table, &[2, 5]);
    }

    #[test]
    fn decodes_repeat_previous_run() {
        let data = symbol_id_table_stream(
            &[(usize::from(3u8), 1), (REPEAT_PREVIOUS_RUN_CODE, 1)],
            &[(0, 1), (1, 1), (0, REPEAT_PREVIOUS_EXTRA_BITS)],
        );
        let mut reader = BitReader::new(&data);
        let table = decode_symbol_id_huffman_table(&mut reader, 4).expect("table");

        assert_code_lengths(&table, &[3, 3, 3, 3]);
    }

    #[test]
    fn decodes_repeat_previous_at_start_as_zero_run() {
        let data = symbol_id_table_stream(
            &[(REPEAT_PREVIOUS_RUN_CODE, 1)],
            &[(0, 1), (0, REPEAT_PREVIOUS_EXTRA_BITS)],
        );
        let mut reader = BitReader::new(&data);
        let table = decode_symbol_id_huffman_table(&mut reader, 3).expect("table");

        assert_code_lengths(&table, &[0, 0, 0]);
    }

    #[test]
    fn decodes_short_zero_run() {
        let data = symbol_id_table_stream(
            &[(REPEAT_ZERO_SHORT_RUN_CODE, 1)],
            &[(0, 1), (0, REPEAT_ZERO_SHORT_EXTRA_BITS)],
        );
        let mut reader = BitReader::new(&data);
        let table = decode_symbol_id_huffman_table(&mut reader, 3).expect("table");

        assert_code_lengths(&table, &[0, 0, 0]);
    }

    #[test]
    fn decodes_long_zero_run() {
        let data = symbol_id_table_stream(
            &[(REPEAT_ZERO_LONG_RUN_CODE, 1)],
            &[(0, 1), (0, REPEAT_ZERO_LONG_EXTRA_BITS)],
        );
        let mut reader = BitReader::new(&data);
        let table = decode_symbol_id_huffman_table(&mut reader, 11).expect("table");

        assert_code_lengths(&table, &[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn clamps_runs_past_symbol_count() {
        let data = symbol_id_table_stream(
            &[(REPEAT_ZERO_LONG_RUN_CODE, 1)],
            &[(0, 1), (0, REPEAT_ZERO_LONG_EXTRA_BITS)],
        );
        let mut reader = BitReader::new(&data);
        let table = decode_symbol_id_huffman_table(&mut reader, 10).expect("table");

        assert_code_lengths(&table, &[0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn rejects_truncated_run_code_lengths() {
        let required_bits = RUN_CODE_COUNT.saturating_mul(usize::from(RUN_CODE_LENGTH_BITS));
        let byte_count = required_bits.saturating_sub(1) / 8;
        let data = vec![0; byte_count];
        let mut reader = BitReader::new(&data);
        let result = decode_symbol_id_huffman_table(&mut reader, 1);

        assert_eq!(
            result,
            Err(Jbig2Error::Truncated("symbol-id Huffman table"))
        );
    }

    #[test]
    fn rejects_truncated_repeat_extra_bits() {
        let data = symbol_id_table_stream(&[(REPEAT_ZERO_LONG_RUN_CODE, 1)], &[(0, 1)]);
        let mut reader = BitReader::new(&data);
        let result = decode_symbol_id_huffman_table(&mut reader, 11);

        assert_eq!(
            result,
            Err(Jbig2Error::Truncated("symbol-id Huffman table"))
        );
    }

    fn assert_code_lengths(table: &SymbolIdHuffmanTable, expected: &[u8]) {
        assert_eq!(table.codes().len(), expected.len());
        assert!(
            table
                .codes()
                .iter()
                .zip(expected.iter().copied())
                .all(|(code, expected_len)| code.codelen == expected_len)
        );
    }

    fn symbol_id_table_stream(run_lengths: &[(usize, u8)], payload: &[(u32, u8)]) -> Vec<u8> {
        let mut bits = Vec::new();
        for run_code in 0..RUN_CODE_COUNT {
            let len = run_lengths
                .iter()
                .find_map(|&(index, len)| (index == run_code).then_some(len))
                .unwrap_or(0);
            push_bits(&mut bits, u32::from(len), RUN_CODE_LENGTH_BITS);
        }
        for &(value, len) in payload {
            push_bits(&mut bits, value, len);
        }
        bits_to_bytes(&bits)
    }
}
