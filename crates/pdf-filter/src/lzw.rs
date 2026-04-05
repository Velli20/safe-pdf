use crate::{bitreader::BitReader, error::FilterError};

/// Clear-table code (resets the string table).
const CLEAR_CODE: u16 = 256;
/// End-of-data code (signals the end of the compressed stream).
const EOD_CODE: u16 = 257;
/// First code available for string-table entries.
const FIRST_CODE: u16 = 258;
/// Maximum code width in bits (PDF spec §7.4.4.2).
const MAX_CODE_WIDTH: u8 = 12;
/// Maximum number of table entries (2^12).
const MAX_TABLE_SIZE: usize = 4096;

/// Decodes LZW-compressed data per PDF specification §7.4.4.
///
/// The `early_change` parameter controls when the code width increases:
/// - `true` (PDF default, EarlyChange=1): code width increases when the
///   next code to be assigned reaches `2^width - 1`.
/// - `false` (EarlyChange=0): code width increases when the next code to
///   be assigned reaches `2^width`.
///
/// # Errors
///
/// Returns [`FilterError::Decompression`] if the stream is malformed.
pub(crate) fn decode(data: &[u8], early_change: bool) -> Result<Vec<u8>, FilterError> {
    let mut reader = BitReader::new(data);
    let mut output = Vec::with_capacity(data.len().saturating_mul(2));

    let mut table = StringTable::new();
    let mut code_width: u8 = 9;

    // The first code must be a clear code per PDF spec.
    let first = reader
        .read_bits(code_width)
        .ok_or_else(|| FilterError::Decompression("LZW: unexpected end of data".into()))?;
    if first != CLEAR_CODE {
        return Err(FilterError::Decompression(
            "LZW: expected clear code at start".into(),
        ));
    }

    // Read the first real code after the clear code.
    let mut prev_code = loop {
        let code = reader
            .read_bits(code_width)
            .ok_or_else(|| FilterError::Decompression("LZW: unexpected end of data".into()))?;
        if code == CLEAR_CODE {
            // Multiple clear codes in a row — just reset and continue.
            table.reset();
            code_width = 9;
            continue;
        }
        if code == EOD_CODE {
            return Ok(output);
        }
        // Must be a single-byte code (0–255) since the table was just cleared.
        if code >= FIRST_CODE {
            return Err(FilterError::Decompression(
                "LZW: invalid code after clear".into(),
            ));
        }
        #[allow(clippy::as_conversions)]
        let byte = code as u8; // safe: code < 258
        output.push(byte);
        break code;
    };

    while let Some(code) = reader.read_bits(code_width) {
        if code == EOD_CODE {
            break;
        }

        if code == CLEAR_CODE {
            table.reset();
            code_width = 9;

            // Read the next real code after the clear.
            prev_code = loop {
                let c = match reader.read_bits(code_width) {
                    Some(c) => c,
                    None => return Ok(output),
                };
                if c == CLEAR_CODE {
                    table.reset();
                    code_width = 9;
                    continue;
                }
                if c == EOD_CODE {
                    return Ok(output);
                }
                if c >= FIRST_CODE {
                    return Err(FilterError::Decompression(
                        "LZW: invalid code after clear".into(),
                    ));
                }
                #[allow(clippy::as_conversions)]
                let byte = c as u8; // safe: c < 258
                output.push(byte);
                break c;
            };
            continue;
        }

        let next_code = table.next_code();

        if code < FIRST_CODE {
            // Single-byte literal.
            #[allow(clippy::as_conversions)]
            let byte = code as u8; // safe: code < 258
            output.push(byte);
            // Add prev_string + first byte of current string to table.
            table.add_entry(prev_code, byte)?;
        } else if code < next_code {
            // Code is already in the table.
            let first_byte = table.first_byte(code)?;
            table.add_entry(prev_code, first_byte)?;
            table.emit(code, &mut output)?;
        } else if code == next_code {
            // Special case: code not yet in table. The string is
            // prev_string + first byte of prev_string.
            let first_byte = if prev_code < FIRST_CODE {
                #[allow(clippy::as_conversions)]
                let b = prev_code as u8; // safe: prev_code < 258
                b
            } else {
                table.first_byte(prev_code)?
            };
            table.add_entry(prev_code, first_byte)?;
            table.emit(code, &mut output)?;
        } else {
            return Err(FilterError::Decompression(format!(
                "LZW: code {code} exceeds next available code {next_code}"
            )));
        }

        prev_code = code;
        code_width = bump_code_width(table.next_code(), code_width, early_change);
    }

    Ok(output)
}

/// Determines the new code width after a table entry was added.
fn bump_code_width(next_code: u16, current_width: u8, early_change: bool) -> u8 {
    if current_width >= MAX_CODE_WIDTH {
        return current_width;
    }
    let trigger = if early_change {
        (1u16 << current_width).saturating_sub(1)
    } else {
        1u16 << current_width
    };
    if next_code >= trigger {
        current_width.saturating_add(1).min(MAX_CODE_WIDTH)
    } else {
        current_width
    }
}

/// The LZW string table.
///
/// Entries 0–255 are implicit single-byte strings. Entries 256 (clear) and
/// 257 (EOD) are reserved. Explicit entries start at index 258.
///
/// Each explicit entry stores `(prefix_code, suffix_byte)` — the string is
/// formed by recursively expanding the prefix and appending the suffix.
struct StringTable {
    /// `(prefix_code, suffix_byte)` for each explicit entry.
    entries: Vec<(u16, u8)>,
}

impl StringTable {
    fn new() -> Self {
        Self {
            entries: Vec::with_capacity(MAX_TABLE_SIZE.saturating_sub(usize::from(FIRST_CODE))),
        }
    }

    fn reset(&mut self) {
        self.entries.clear();
    }

    /// The next code that will be assigned.
    #[allow(clippy::as_conversions)]
    fn next_code(&self) -> u16 {
        // entries.len() is at most MAX_TABLE_SIZE - FIRST_CODE = 3838, fits in u16.
        FIRST_CODE.saturating_add(self.entries.len() as u16)
    }

    /// Add a new table entry: the string for `prefix` followed by `suffix`.
    fn add_entry(&mut self, prefix: u16, suffix: u8) -> Result<(), FilterError> {
        if self.entries.len().saturating_add(usize::from(FIRST_CODE)) < MAX_TABLE_SIZE {
            self.entries.push((prefix, suffix));
        }
        // Silently ignore if table is full (spec says no more entries until clear).
        Ok(())
    }

    /// Returns the first byte of the string represented by `code`.
    fn first_byte(&self, code: u16) -> Result<u8, FilterError> {
        let mut c = code;
        // Walk prefix chain until we reach a single-byte code.
        loop {
            if c < FIRST_CODE {
                #[allow(clippy::as_conversions)]
                return Ok(c as u8); // safe: c < 258
            }
            let idx = usize::from(c.saturating_sub(FIRST_CODE));
            let &(prefix, _) = self.entries.get(idx).ok_or_else(|| {
                FilterError::Decompression(format!("LZW: invalid table index {c}"))
            })?;
            c = prefix;
        }
    }

    /// Emit the full string for `code` into `output`.
    fn emit(&self, code: u16, output: &mut Vec<u8>) -> Result<(), FilterError> {
        if code < FIRST_CODE {
            #[allow(clippy::as_conversions)]
            let byte = code as u8; // safe: code < 258
            output.push(byte);
            return Ok(());
        }

        // Walk the prefix chain, collecting bytes in reverse order.
        let start = output.len();
        let mut c = code;
        loop {
            if c < FIRST_CODE {
                #[allow(clippy::as_conversions)]
                let byte = c as u8; // safe: c < 258
                output.push(byte);
                break;
            }
            let idx = usize::from(c.saturating_sub(FIRST_CODE));
            let &(prefix, suffix) = self.entries.get(idx).ok_or_else(|| {
                FilterError::Decompression(format!("LZW: invalid table index {c}"))
            })?;
            output.push(suffix);
            c = prefix;
        }
        // Reverse the portion we just emitted to get the correct order.
        if let Some(slice) = output.get_mut(start..) {
            slice.reverse();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: pack a sequence of variable-width codes into bytes, MSB-first.
    fn pack_codes(codes: &[(u16, u8)]) -> Vec<u8> {
        let mut bits = Vec::new();
        for &(code, width) in codes {
            for i in (0..width).rev() {
                bits.push((code >> i) & 1 != 0);
            }
        }
        let mut bytes = Vec::new();
        for chunk in bits.chunks(8) {
            let mut byte = 0u8;
            for (i, &bit) in chunk.iter().enumerate() {
                if bit {
                    byte |= 1 << (7 - i);
                }
            }
            bytes.push(byte);
        }
        bytes
    }

    #[test]
    fn decode_single_byte_stream() {
        // Clear(256) + literal 'A'(65) + EOD(257), all at 9-bit width.
        let data = pack_codes(&[(256, 9), (65, 9), (257, 9)]);
        let result = decode(&data, true).expect("decode failed");
        assert_eq!(result, b"A");
    }

    #[test]
    fn decode_repeated_bytes() {
        // "AAAA" — after clear, emit A, A, then code 258 (AA), then EOD.
        // Codes: clear(256), A(65), A(65), 258, EOD(257) all at 9 bits.
        let data = pack_codes(&[(256, 9), (65, 9), (65, 9), (258, 9), (257, 9)]);
        let result = decode(&data, true).expect("decode failed");
        assert_eq!(result, b"AAAA");
    }

    #[test]
    fn decode_known_sequence() {
        // Encode "ABCABC":
        //   clear(256)
        //   A(65)         -> table[258] = prev+first(A) => won't be added yet
        //   B(66)         -> table[258] = AB
        //   C(67)         -> table[259] = BC
        //   258           -> emit AB, table[260] = CA
        //   C(67)         -> emit C, table[261] = ABC (wait: prev was 258="AB", first(C)=C)
        //   EOD(257)
        //
        // Actually, let's trace carefully:
        //   prev=A(65)
        //   code=B(66): table[258]=(65,'B')="AB", emit B, prev=66
        //   code=C(67): table[259]=(66,'C')="BC", emit C, prev=67
        //   code=258:   first_byte(258)='A', table[260]=(67,'A')="CA", emit "AB", prev=258
        //   code=C(67): first_byte(67)='C', table[261]=(258,'C')="ABC", emit C, prev=67
        //   EOD
        //   Output: A B C AB C = "ABCABC"
        let data = pack_codes(&[
            (256, 9),
            (65, 9),
            (66, 9),
            (67, 9),
            (258, 9),
            (67, 9),
            (257, 9),
        ]);
        let result = decode(&data, true).expect("decode failed");
        assert_eq!(result, b"ABCABC");
    }

    #[test]
    fn decode_special_case_code_equals_next() {
        // The KwKwK case: code == next_code.
        // Input: "ABAB"
        //   clear(256)
        //   A(65)          prev=65
        //   B(66)          table[258]=(65,'B')="AB", prev=66
        //   258            code==next_code? next_code=259, so 258<259, it's in table.
        //                  first_byte(258)='A', table[259]=(66,'A')="BA", emit "AB", prev=258
        //
        // That gives "ABAB" — but that doesn't exercise the special case.
        // Let's construct the KwKwK case: "ABCABCABC" with code==next.
        //   clear(256)
        //   A(65)          prev=65
        //   B(66)          table[258]=(65,'B')="AB", prev=66
        //   258            first_byte(258)='A', table[259]=(66,'A')="BA", emit "AB", prev=258
        //   259            first_byte(259)='B', table[260]=(258,'B')="ABB", emit "BA", prev=259
        //
        // That's "ABAB BA" = "ABABBA". Not the KwKwK case either.
        //
        // Classic KwKwK: "ABABAB"
        //   clear(256)
        //   A(65)          prev=65
        //   B(66)          table[258]=(65,'B')="AB", prev=66
        //   258            first_byte(258)='A', table[259]=(66,'A')="BA", emit "AB", prev=258
        //   260            code=260==next_code(260)!
        //                  first_byte(prev=258)='A', add (258,'A')=table[260]="ABA"
        //                  emit 260 = "ABA"
        //   EOD(257)
        //   Output: A B AB ABA = "ABABABA" (7 bytes)
        let data = pack_codes(&[(256, 9), (65, 9), (66, 9), (258, 9), (260, 9), (257, 9)]);
        let result = decode(&data, true).expect("decode failed");
        assert_eq!(result, b"ABABABA");
    }

    #[test]
    fn decode_empty_after_clear() {
        // Clear followed immediately by EOD.
        let data = pack_codes(&[(256, 9), (257, 9)]);
        let result = decode(&data, true).expect("decode failed");
        assert!(result.is_empty());
    }

    #[test]
    fn decode_multiple_clears() {
        // Clear, A, Clear, B, EOD
        let data = pack_codes(&[(256, 9), (65, 9), (256, 9), (66, 9), (257, 9)]);
        let result = decode(&data, true).expect("decode failed");
        assert_eq!(result, b"AB");
    }

    #[test]
    fn decode_missing_initial_clear_is_error() {
        // Start with a literal instead of clear code.
        let data = pack_codes(&[(65, 9), (257, 9)]);
        let result = decode(&data, true);
        assert!(result.is_err());
    }

    #[test]
    fn decode_early_change_false() {
        // With early_change=false, code width should bump at 512 (not 511).
        // We need 254 entries (258..511) to fill the 9-bit range.
        // After adding entry 511, next_code=512, trigger=512, so width bumps to 10.
        //
        // Build a stream that creates enough entries to cross the boundary:
        // clear(256), then 255 distinct single-byte codes (0..254), then one more.
        // Each new code after the first adds a table entry.
        let mut codes: Vec<(u16, u8)> = Vec::new();
        codes.push((256, 9)); // clear
        // Emit bytes 0..255 — each pair adds a table entry
        for i in 0u16..=254 {
            codes.push((i, 9));
        }
        // At this point, table has entries 258..511 (254 entries), next_code=512.
        // With early_change=false, width should NOW be 10 (bumped when next_code==512).
        // With early_change=true, width would have bumped at next_code==511.
        //
        // The 256th literal (code 255) was read at 9-bit width.
        // After it's processed, next_code = 258 + 254 = 512, so bump to 10.
        // Next code must be read at 10-bit width.
        codes.push((257, 10)); // EOD at 10-bit width

        let data = pack_codes(&codes);
        let result = decode(&data, false).expect("decode failed");
        // Should output bytes 0, 1, 2, ..., 254
        let expected: Vec<u8> = (0u8..=254).collect();
        assert_eq!(result, expected);
    }

    #[test]
    fn decode_early_change_true_width_bump() {
        // With early_change=true, code width bumps at next_code==511 (2^9 - 1).
        // Need 253 entries (258..510) to reach next_code=511.
        // 253 entries means 254 codes after clear (first code doesn't add an entry,
        // each subsequent code adds one: 253 entries from 254 codes after the first).
        let mut codes: Vec<(u16, u8)> = Vec::new();
        codes.push((256, 9)); // clear
        // Emit 254 distinct bytes: first one (byte 0) sets prev, next 253 each add an entry.
        for i in 0u16..=253 {
            codes.push((i, 9));
        }
        // Table now has 253 entries (258..510), next_code=511.
        // With early_change=true, trigger = 2^9 - 1 = 511, so width bumps to 10 now.
        // The next code must be read at 10-bit width.
        codes.push((254, 10)); // one more literal at 10 bits
        codes.push((257, 10)); // EOD at 10-bit width

        let data = pack_codes(&codes);
        let result = decode(&data, true).expect("decode failed");
        let expected: Vec<u8> = (0u8..=254).collect();
        assert_eq!(result, expected);
    }

    #[test]
    fn bump_code_width_early_change_true() {
        // At width 9, trigger = 511. next_code < 511 → no bump.
        assert_eq!(bump_code_width(510, 9, true), 9);
        // next_code == 511 → bump.
        assert_eq!(bump_code_width(511, 9, true), 10);
        // At max width 12, never bump.
        assert_eq!(bump_code_width(4095, 12, true), 12);
    }

    #[test]
    fn bump_code_width_early_change_false() {
        // At width 9, trigger = 512. next_code < 512 → no bump.
        assert_eq!(bump_code_width(511, 9, false), 9);
        // next_code == 512 → bump.
        assert_eq!(bump_code_width(512, 9, false), 10);
    }
}
