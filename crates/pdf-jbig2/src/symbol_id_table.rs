//! The run-coded symbol identifier table of a Huffman text region.
//!
//! T.88 section 7.4.3.1.7 stores one code length per symbol, compressed with a
//! small Huffman table of its own whose codes either give a length directly or
//! repeat a length across a run of symbols.

use crate::{
    Jbig2Error, Result,
    document::DecodeBudget,
    huffman::{HuffmanDecoder, HuffmanLine, HuffmanRange, HuffmanTable},
};

/// Number of run codes the table's own alphabet defines.
const RUN_CODE_COUNT: usize = 35;
/// Run code repeating the previous length.
const REPEAT_PREVIOUS: i64 = 32;
/// Run code assigning a short run of unused symbols.
const SHORT_ZERO_RUN: i64 = 33;
/// Run code assigning a long run of unused symbols.
const LONG_ZERO_RUN: i64 = 34;

/// Read the symbol identifier code table preceding a Huffman text region's
/// coded data, leaving the decoder byte aligned on the data that follows.
pub(crate) fn parse(
    decoder: &mut HuffmanDecoder<'_>,
    symbols: usize,
    budget: &mut DecodeBudget,
) -> Result<HuffmanTable> {
    let run_codes = parse_run_code_table(decoder, budget)?;
    let lengths = decode_code_lengths(decoder, &run_codes, symbols, budget)?;
    decoder.align_to_byte()?;
    build_table(&lengths, budget)
}

/// Read the thirty-five four-bit run code lengths and build their table.
fn parse_run_code_table(
    decoder: &mut HuffmanDecoder<'_>,
    budget: &mut DecodeBudget,
) -> Result<HuffmanTable> {
    let mut lines = Vec::new();
    lines.try_reserve_exact(RUN_CODE_COUNT)?;
    for value in 0..RUN_CODE_COUNT {
        let prefix_bits = u8::try_from(decoder.read_bits(4, budget)?)
            .map_err(|_| Jbig2Error::Overflow("run code length"))?;
        lines.push(symbol_line(prefix_bits, value)?);
    }
    HuffmanTable::from_lines(&lines, budget)
}

/// Expand the run codes into one code length per symbol.
fn decode_code_lengths(
    decoder: &mut HuffmanDecoder<'_>,
    run_codes: &HuffmanTable,
    symbols: usize,
    budget: &mut DecodeBudget,
) -> Result<Vec<u8>> {
    let mut lengths: Vec<u8> = Vec::new();
    lengths.try_reserve_exact(symbols)?;
    while lengths.len() < symbols {
        let code = decoder
            .decode_integer(run_codes, budget)?
            .value("symbol identifier run code")?;
        let (length, repeats) = match code {
            REPEAT_PREVIOUS => {
                let previous = *lengths
                    .last()
                    .ok_or(Jbig2Error::InvalidResource("repeat before any code length"))?;
                (previous, decoder.read_bits(2, budget)?.saturating_add(3))
            }
            SHORT_ZERO_RUN => (0, decoder.read_bits(3, budget)?.saturating_add(3)),
            LONG_ZERO_RUN => (0, decoder.read_bits(7, budget)?.saturating_add(11)),
            direct if (0..REPEAT_PREVIOUS).contains(&direct) => (
                u8::try_from(direct).map_err(|_| Jbig2Error::Overflow("code length"))?,
                1,
            ),
            _ => return Err(Jbig2Error::InvalidResource("symbol identifier run code")),
        };
        let repeats = usize::try_from(repeats).map_err(|_| Jbig2Error::Overflow("run length"))?;
        if lengths.len().saturating_add(repeats) > symbols {
            return Err(Jbig2Error::InvalidResource(
                "symbol identifier run exceeds the symbol count",
            ));
        }
        lengths.resize(lengths.len().saturating_add(repeats), length);
    }
    Ok(lengths)
}

/// Build the canonical identifier table from its per-symbol code lengths.
fn build_table(lengths: &[u8], budget: &mut DecodeBudget) -> Result<HuffmanTable> {
    let mut lines = Vec::new();
    lines.try_reserve_exact(lengths.len())?;
    for (value, &prefix_bits) in lengths.iter().enumerate() {
        lines.push(symbol_line(prefix_bits, value)?);
    }
    HuffmanTable::from_lines(&lines, budget)
}

/// A line decoding to exactly `value`, carrying no range bits.
fn symbol_line(prefix_bits: u8, value: usize) -> Result<HuffmanLine> {
    Ok(HuffmanLine {
        prefix_bits,
        range: HuffmanRange::Upper {
            base: i64::try_from(value).map_err(|_| Jbig2Error::Overflow("symbol identifier"))?,
            bits: 0,
        },
    })
}
