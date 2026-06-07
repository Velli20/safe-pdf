use crate::huffman::{HuffmanValue, StandardHuffmanDecoder};

/// Append a value encoded with a standard Huffman decoder to `bits`.
///
/// This mirrors ITU-T T.88 / ISO/IEC 14492 Annex B only for tests that need to
/// build compact JBIG2 fixture streams.
pub(crate) fn encode_standard_huffman_value(
    bits: &mut Vec<bool>,
    table: &StandardHuffmanDecoder,
    value: HuffmanValue,
) -> Option<()> {
    let (code, codelen, extra, extra_len) = bits_for_value(table, value)?;
    push_bits(bits, code, codelen);
    push_bits(bits, extra, extra_len);
    Some(())
}

/// Return the Huffman prefix and extra bits for a desired decoded value.
///
/// The tuple is `(prefix, prefix_len, extra_bits, extra_len)`, matching the
/// Annex B prefix-plus-range encoding used by tests.
pub(crate) fn bits_for_value(
    table: &StandardHuffmanDecoder,
    value: HuffmanValue,
) -> Option<(u32, u8, u32, u8)> {
    match value {
        HuffmanValue::OutOfBand => table
            .codes()
            .last()
            .map(|code| (code.code, code.codelen, 0, 0)),
        HuffmanValue::Value(value) => {
            for index in 0..table.codes().len() {
                let (entry, code) = table.table_row(index)?;
                let range_span = if entry.range_len == 0 {
                    1i32
                } else {
                    1i32.checked_shl(u32::from(entry.range_len))?
                };
                let range_tail = range_span.checked_sub(1)?;
                let subtracts = lower_open_range_matches(table, index);
                let matches = if subtracts {
                    let high = entry.range_low;
                    let low = high.checked_sub(range_tail)?;
                    value >= low && value <= high
                } else {
                    let low = entry.range_low;
                    let high = low.checked_add(range_tail)?;
                    value >= low && value <= high
                };
                if !matches {
                    continue;
                }
                let extra = if subtracts {
                    u32::try_from(entry.range_low.checked_sub(value)?).ok()?
                } else {
                    u32::try_from(value.checked_sub(entry.range_low)?).ok()?
                };
                return Some((code.code, code.codelen, extra, entry.range_len));
            }
            None
        }
    }
}

/// Pack a bit vector into bytes using the JBIG2 most-significant-bit order.
pub(crate) fn bits_to_bytes(bits: &[bool]) -> Vec<u8> {
    let mut bytes = Vec::new();
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
    bytes
}

/// Append `width` most-significant bits from `value` to `bits`.
pub(crate) fn push_bits(bits: &mut Vec<bool>, value: u32, width: u8) {
    for shift in (0..u32::from(width)).rev() {
        bits.push(value.checked_shr(shift).unwrap_or(0) & 1 != 0);
    }
}

/// Return whether `index` is the lower open-ended range row for `table`.
///
/// Tests derive this from the visible row behavior because the production
/// helper is intentionally private to the decoder.
fn lower_open_range_matches(table: &StandardHuffmanDecoder, index: usize) -> bool {
    let Some((entry, _)) = table.table_row(index) else {
        return false;
    };
    entry.range_len == 32 && entry.range_low <= 0
}
