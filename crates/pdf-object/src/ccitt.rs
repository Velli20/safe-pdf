//! Pure-Rust decoder for the CCITTFaxDecode stream filter (PDF §7.4.6).
//!
//! Supports three encoding modes selected by the `K` parameter:
//!
//! | K value | Encoding |
//! |---------|----------|
//! | `K = 0` | Group 3, one-dimensional (Modified Huffman / T.4 1D) |
//! | `K > 0` | Group 3, two-dimensional (T.4 2D) |
//! | `K < 0` | Group 4, two-dimensional (MMR / T.6) |
//!
//! No external crate dependencies are required; all Huffman tables are
//! embedded as `const` data taken from ITU-T T.4 (Appendix A) and T.6.
//!
//! # Errors
//!
//! Decompression errors are reported through [`CcittDecodeError`].  Truncated
//! or damaged streams are decoded best-effort (matching PDFium behaviour).

use thiserror::Error;

use crate::{
    bitreader::BitReader,
    ccitt_fax_params::CCITTFaxParams,
    ccitt_tables::{BLACK_RUN_INS, ONE_LEAD_POS, WHITE_RUN_INS},
    error::ObjectError,
};

/// An error that occurred during CCITTFaxDecode decompression.
///
/// This type is `#[non_exhaustive]` to allow adding new variants in the future
/// without breaking downstream code.
#[non_exhaustive]
#[derive(Debug, Error, PartialEq)]
pub enum CcittDecodeError {
    /// The column count is zero, which is not a valid image width.
    #[error("CCITTFaxDecode: zero column count")]
    ZeroColumns,
    /// The bit stream ended before the row was fully decoded.
    #[error("CCITTFaxDecode: unexpected end of stream")]
    UnexpectedEof,
    /// No matching Huffman codeword could be found in the run-length tables.
    #[error("CCITTFaxDecode: invalid Huffman code")]
    InvalidCode,
    /// 2D decoding: the current position regressed (monotonicity constraint violated).
    #[error("CCITTFaxDecode: 2D monotonicity violated")]
    MonotonicityViolated,
    /// 2D decoding: an unknown extension codeword was encountered.
    #[error("CCITTFaxDecode: unknown extension codeword")]
    UnknownExtensionCode,
    /// A wrapped [`ObjectError`] from an upstream PDF object operation.
    #[error("CCITTFaxDecode: {0}")]
    ObjectError(#[from] ObjectError),
}

/// Returns the MSB-first position of the leading set bit of `byte`
/// (0 = MSB, 7 = LSB).  Returns 8 when `byte == 0`.
#[inline]
fn one_lead_pos(byte: u8) -> usize {
    usize::from(ONE_LEAD_POS.get(usize::from(byte)).copied().unwrap_or(8))
}

/// Returns the color (true = white, false = black) of pixel `pos` in `row`.
/// Out-of-bounds reads return white (imaginary padding).
#[inline]
fn ref_pixel(row: &[u8], pos: usize) -> bool {
    row.get(pos / 8)
        .is_none_or(|&b| (b >> (7 - pos % 8)) & 1 != 0)
}

/// Set pixels in `[start, end)` to black (0) in `row`.
/// `row` must be at least `(columns + 7) / 8` bytes long.
fn fill_bits(row: &mut [u8], columns: usize, start: usize, end: usize) {
    let end = end.min(columns);
    if start >= end {
        return;
    }

    let first_byte = start / 8;
    let last_byte = (end - 1) / 8; // safe: end > 0

    let bit_start = start % 8;
    let bit_end = (end - 1) % 8;

    if first_byte == last_byte {
        // Both endpoints in the same byte.
        let mask_hi = 0xffu8 >> bit_start;
        let mask_lo = if bit_end < 7 {
            !(0xffu8 >> (bit_end + 1))
        } else {
            0xff
        };
        if let Some(b) = row.get_mut(first_byte) {
            *b &= !(mask_hi & mask_lo);
        }
        return;
    }

    // Clear the tail of the first byte.
    if let Some(b) = row.get_mut(first_byte) {
        *b &= !(0xffu8 >> bit_start);
    }
    // Clear whole middle bytes.
    if let Some(middle) = row.get_mut(first_byte + 1..last_byte) {
        middle.fill(0x00);
    }
    // Clear the head of the last byte.
    let mask_last = if bit_end < 7 {
        !(0xffu8 >> (bit_end + 1))
    } else {
        0xff
    };
    if let Some(b) = row.get_mut(last_byte) {
        *b &= !mask_last;
    }
}

/// Returns the position of the first bit equal to `bit` in `row[start_pos..max_pos]`,
/// or `max_pos` if not found.
fn find_bit(row: &[u8], max_pos: usize, start_pos: usize, bit: bool) -> usize {
    let start_pos = start_pos.min(max_pos);
    let bit_xor: u8 = if bit { 0x00 } else { 0xff };

    let mut byte_pos;

    // Handle the partial leading byte.
    let bit_offset = start_pos % 8;
    if bit_offset != 0 {
        let bp = start_pos / 8;
        if let Some(&b) = row.get(bp) {
            let data = (b ^ bit_xor) & (0xffu8 >> bit_offset);
            if data != 0 {
                return (bp * 8 + one_lead_pos(data)).min(max_pos);
            }
        }
        byte_pos = start_pos.div_ceil(8);
    } else {
        byte_pos = start_pos / 8;
    }

    let max_byte = max_pos.div_ceil(8);
    let skip_byte: u8 = if bit { 0x00 } else { 0xff };

    // Skip 8 bytes at a time when there is nothing to find.
    while byte_pos + 8 <= max_byte {
        match row.get(byte_pos..byte_pos + 8) {
            Some(chunk) if chunk.iter().all(|&b| b == skip_byte) => {
                byte_pos += 8;
            }
            _ => break,
        }
    }

    while byte_pos < max_byte {
        if let Some(&b) = row.get(byte_pos) {
            let data = b ^ bit_xor;
            if data != 0 {
                return (byte_pos * 8 + one_lead_pos(data)).min(max_pos);
            }
        }
        byte_pos += 1;
    }

    max_pos
}

/// Locate reference pixels b1 and b2 for the 2D row decoder.
///
/// * `a0` – current position on the coding line (`None` = before first pixel).
/// * `a0color` – color of the current coding line at `a0`.
///
/// Returns `(b1, b2)` where each may equal `columns` when not found.
fn find_b1_b2(ref_row: &[u8], columns: usize, a0: Option<usize>, a0color: bool) -> (usize, usize) {
    // Color of the reference line at position a0 (white when before first pixel).
    let first_bit = match a0 {
        None => true,
        Some(pos) => ref_pixel(ref_row, pos),
    };

    let search_start = a0.map_or(0, |pos| pos.saturating_add(1));
    let mut b1 = find_bit(ref_row, columns, search_start, !first_bit);

    if b1 >= columns {
        return (columns, columns);
    }

    // If the reference line at a0 has the same color as the coding line at a0,
    // the first transition found above leads to the wrong color.  Advance once
    // more so that b1 ends up at the nearest opposite-color transition.
    let mut cur_first_bit = first_bit;
    if first_bit != a0color {
        b1 = find_bit(ref_row, columns, b1 + 1, first_bit);
        cur_first_bit = !cur_first_bit;
    }

    if b1 >= columns {
        return (columns, columns);
    }

    let b2 = find_bit(ref_row, columns, b1 + 1, cur_first_bit);
    (b1, b2)
}

/// Decode one Huffman-encoded run length from `reader`.
/// Returns the run length, or `None` on truncated / invalid stream.
fn get_run(ins: &[u8], reader: &mut BitReader<'_>) -> Option<u16> {
    let mut code: u32 = 0;
    let mut ins_off: usize = 0;

    loop {
        let count = *ins.get(ins_off)?;
        ins_off += 1;

        if count == 0xff {
            return None;
        }

        let bit = reader.next_bit()?;
        code = (code << 1) | u32::from(bit);

        let next_off = ins_off.saturating_add(usize::from(count) * 3);
        while ins_off < next_off {
            let entry_code = ins.get(ins_off).copied()?;
            if u32::from(entry_code) == code {
                let lo = u16::from(ins.get(ins_off + 1).copied()?);
                let hi = u16::from(ins.get(ins_off + 2).copied()?);
                return Some(lo | (hi << 8));
            }
            ins_off += 3;
        }
    }
}

/// Decode a complete run-length sequence (one or more makeup codes followed
/// by a single terminating code).
///
/// `color`: `true` = white run, `false` = black run.
///
/// # Errors
///
/// Returns [`CcittDecodeError::InvalidCode`] when no matching Huffman codeword
/// is found (including cases where the stream is exhausted mid-code).
fn decode_run_seq(reader: &mut BitReader<'_>, color: bool) -> Result<usize, CcittDecodeError> {
    let ins = if color { WHITE_RUN_INS } else { BLACK_RUN_INS };
    let mut total: usize = 0;
    const CCITT_TERMINATING_RUN_LIMIT: usize = 64;

    loop {
        let run = usize::from(get_run(ins, reader).ok_or(CcittDecodeError::InvalidCode)?);
        total = total.saturating_add(run);
        if run < CCITT_TERMINATING_RUN_LIMIT {
            return Ok(total);
        }
        // Makeup code: keep reading for the terminating code.
    }
}

/// Skip an end-of-line marker if one is present.
///
/// An EOL consists of ≥ 11 leading zero-bits followed by a one-bit (12 bits
/// total for T.4 EOL).  If fewer than 12 bits are consumed before the
/// one-bit, the position is reset (no EOL was present).
fn skip_eol(reader: &mut BitReader<'_>) {
    let start = reader.pos();
    loop {
        match reader.next_bit() {
            None => return,
            Some(false) => {} // consume zero-bits
            Some(true) => {
                if reader.pos() - start <= 11 {
                    reader.set_pos(start); // too few zeros – not a real EOL
                }
                return;
            }
        }
    }
}

// ─── 2D opcode decoder ───────────────────────────────────────────────────────

/// A decoded T.4 / T.6 two-dimensional scan-line opcode.
///
/// Each variant is decoded from the bit stream by [`read_2d_mode`] and drives
/// the main dispatch loop inside [`decode_2d_row`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TwoDMode {
    /// Vertical-right mode: `a1 = b1 + delta` where `delta ∈ {0, 1, 2, 3}`.
    VerticalRight(usize),
    /// Vertical-left mode: `a1 = b1 − delta` where `delta ∈ {1, 2, 3}`.
    VerticalLeft(usize),
    /// Horizontal mode: two explicit run-length sequences follow in the bit stream.
    Horizontal,
    /// Pass mode: the coding line "passes" b2; advance `a0` to `b2` without changing color.
    Pass,
    /// End-of-facsimile-block first marker (T.6 §4.2.8).
    ///
    /// The 3 padding bits that complete the first EOL codeword have already
    /// been consumed.  The decoder continues the row loop to process the
    /// second EOFB marker before the stream is exhausted.
    EndOfBlock,
    /// Extension or unknown codeword.
    ///
    /// The 5 padding bits of the extension codeword have already been consumed.
    /// The decoder should terminate the current row.
    Extension,
}

/// Decode one T.4 / T.6 two-dimensional scan-line opcode from `reader`.
///
/// Reads the minimum number of bits needed to identify the mode.  For
/// [`TwoDMode::EndOfBlock`] and [`TwoDMode::Extension`], the remaining bits
/// of the fixed-width codeword (3 and 5 bits respectively) are also consumed
/// before returning, so the caller does not need to skip them.
///
/// Returns `None` if the bit stream is exhausted before a complete codeword
/// can be read.
fn read_2d_mode(reader: &mut BitReader<'_>) -> Option<TwoDMode> {
    // Bit pattern table (ITU-T T.4 Table 1):
    //   1          → V(0)
    //   011        → V(+1)
    //   010        → V(-1)
    //   001        → H mode
    //   0001       → Pass
    //   000011 x   → V(+2) / V(-2)
    //   0000010 x  → V(-3) / V(+3)   (bit=1 → +3)
    //   0000001    → EOFB first EOL  (+ 3 padding bits consumed here)
    //   0000000    → Extension        (+ 5 padding bits consumed here)

    if reader.next_bit()? {
        return Some(TwoDMode::VerticalRight(0)); // 1
    }
    let bit1 = reader.next_bit()?;
    let bit2 = reader.next_bit()?;

    if bit1 {
        // 01x → V(+1) or V(-1)
        return Some(if bit2 {
            TwoDMode::VerticalRight(1)
        } else {
            TwoDMode::VerticalLeft(1)
        });
    }
    if bit2 {
        // 001 → H mode
        return Some(TwoDMode::Horizontal);
    }

    let bit3 = reader.next_bit()?;
    if bit3 {
        // 0001 → Pass
        return Some(TwoDMode::Pass);
    }

    let bit4 = reader.next_bit()?;
    let bit5 = reader.next_bit()?;

    if bit4 {
        // 000011 x → V(+2) or V(-2)
        return Some(if bit5 {
            TwoDMode::VerticalRight(2)
        } else {
            TwoDMode::VerticalLeft(2)
        });
    }
    if bit5 {
        // 0000010 x → V(-3) or V(+3)
        let bit6 = reader.next_bit()?;
        return Some(if bit6 {
            TwoDMode::VerticalRight(3)
        } else {
            TwoDMode::VerticalLeft(3)
        });
    }

    // 000000 x
    let bit6 = reader.next_bit()?;
    if bit6 {
        // 0000001 → first EOL of EOFB; consume 3 more bits to complete the
        // 12-bit EOL pattern from the 7 bits already read (7 + 3 + a leading 1 = 11).
        reader.skip_bits(3);
        Some(TwoDMode::EndOfBlock)
    } else {
        // 0000000 → extension codeword; consume 5 more bits.
        reader.skip_bits(5);
        Some(TwoDMode::Extension)
    }
}

/// Internal decoder state for a single CCITTFaxDecode stream.
///
/// Encapsulates the bit reader, row buffers, and configuration needed to
/// decode rows sequentially.  Created and consumed by [`decode`].
struct CcittDecoder<'a> {
    reader: BitReader<'a>,
    ref_row: Vec<u8>,
    row_buf: Vec<u8>,
    columns: usize,
    k: i32,
    end_of_line: bool,
    byte_align: bool,
    black_is1: bool,
    /// Maximum number of damaged rows before aborting.
    /// `0` means tolerate all damaged rows (PDF spec §7.4.6).
    damaged_rows_before_error: u32,
}

impl<'a> CcittDecoder<'a> {
    /// Construct a new decoder from raw stream data and CCITT parameters.
    ///
    /// # Errors
    ///
    /// Returns [`CcittDecodeError::ZeroColumns`] if `params.columns` is zero.
    fn new(data: &'a [u8], params: &CCITTFaxParams) -> Result<Self, CcittDecodeError> {
        if params.columns == 0 {
            return Err(CcittDecodeError::ZeroColumns);
        }
        let row_bytes = params.columns.div_ceil(8);

        Ok(Self {
            reader: BitReader::new(data),
            ref_row: vec![0xff; row_bytes],
            row_buf: vec![0xff; row_bytes],
            columns: params.columns,
            k: params.k,
            end_of_line: params.end_of_line,
            byte_align: params.encoded_byte_align,
            black_is1: params.black_is1,
            damaged_rows_before_error: params.damaged_rows_before_error,
        })
    }

    /// Decode all rows and return the concatenated output buffer.
    ///
    /// The caller specifies `max_rows`; `0` means decode until data exhaustion.
    /// Row-level decode errors (truncated data, invalid codes, etc.) are treated
    /// as damaged rows: the partial row is included in the output and decoding
    /// continues.  If `damaged_rows_before_error` is non-zero and the number of
    /// damaged rows exceeds that limit, the error is returned.
    fn decode_all(&mut self, max_rows: usize) -> Result<Vec<u8>, CcittDecodeError> {
        let row_bytes = self.columns.div_ceil(8);
        let mut output: Vec<u8> =
            Vec::with_capacity(max_rows.saturating_mul(row_bytes).max(row_bytes * 16));
        let mut decoded_rows: usize = 0;
        let mut damaged_rows: u32 = 0;

        loop {
            if max_rows > 0 && decoded_rows >= max_rows {
                break;
            }

            skip_eol(&mut self.reader);

            if self.reader.exhausted() {
                break;
            }

            self.row_buf.fill(0xff);

            if let Err(e) = self.decode_next_row() {
                // Row-level errors: include partial row, then decide whether to
                // continue or abort based on damaged_rows_before_error.
                match e {
                    CcittDecodeError::UnexpectedEof => {
                        // Stream ended mid-row — include partial data, stop.
                        output.extend_from_slice(&self.row_buf);
                        break;
                    }
                    CcittDecodeError::ObjectError(_) | CcittDecodeError::ZeroColumns => {
                        return Err(e);
                    }
                    _ => {
                        // InvalidCode, MonotonicityViolated, UnknownExtensionCode, etc.
                        damaged_rows = damaged_rows.saturating_add(1);
                        if self.damaged_rows_before_error > 0
                            && damaged_rows > self.damaged_rows_before_error
                        {
                            return Err(e);
                        }
                        // Include whatever was decoded for this row and continue.
                    }
                }
            }

            if self.end_of_line {
                skip_eol(&mut self.reader);
            }

            if self.byte_align {
                let cur = self.reader.pos();
                let aligned = (cur + 7) & !7;
                if cur < aligned && !self.reader.try_align_to_byte() {
                    self.byte_align = false;
                }
            }

            output.extend_from_slice(&self.row_buf);
            decoded_rows += 1;
        }

        if self.black_is1 {
            output.iter_mut().for_each(|b| *b ^= 0xff);
        }

        Ok(output)
    }

    /// Dispatch a single row based on the `K` parameter.
    fn decode_next_row(&mut self) -> Result<(), CcittDecodeError> {
        match self.k.cmp(&0) {
            std::cmp::Ordering::Less => {
                self.decode_2d_row()?;
                self.ref_row.copy_from_slice(&self.row_buf);
            }
            std::cmp::Ordering::Equal => {
                self.decode_1d_row()?;
            }
            std::cmp::Ordering::Greater => {
                let use_1d = self.reader.next_bit().unwrap_or(true);
                if use_1d {
                    self.decode_1d_row()?;
                } else {
                    self.decode_2d_row()?;
                }
                self.ref_row.copy_from_slice(&self.row_buf);
            }
        }
        Ok(())
    }

    /// Decode one Group 3 1D scan line into `self.row_buf`.
    ///
    /// `row_buf` must be pre-filled with `0xFF` (all white).  Black runs are
    /// written by calling [`fill_bits`].
    ///
    /// # Errors
    ///
    /// Returns [`CcittDecodeError::UnexpectedEof`] when the stream ends before
    /// the row is complete, and [`CcittDecodeError::InvalidCode`] when a run
    /// cannot be decoded.
    fn decode_1d_row(&mut self) -> Result<(), CcittDecodeError> {
        let mut color = true; // T.4 always starts with a white run
        let mut startpos: usize = 0;

        loop {
            if self.reader.exhausted() {
                return Err(CcittDecodeError::UnexpectedEof);
            }

            let run_len = decode_run_seq(&mut self.reader, color)?;

            if !color {
                fill_bits(
                    &mut self.row_buf,
                    self.columns,
                    startpos,
                    startpos.saturating_add(run_len),
                );
            }

            startpos = startpos.saturating_add(run_len);
            if startpos >= self.columns {
                return Ok(());
            }
            color = !color;
        }
    }

    /// Decode one Group 4 (T.6 / MMR) or Group 3 2D scan line into `self.row_buf`.
    ///
    /// `row_buf` must be pre-filled with `0xFF` (all white).
    fn decode_2d_row(&mut self) -> Result<(), CcittDecodeError> {
        let mut a0: Option<usize> = None;
        let mut a0color = true; // white

        loop {
            if self.reader.exhausted() {
                return Err(CcittDecodeError::UnexpectedEof);
            }

            let (b1, b2) = find_b1_b2(&self.ref_row, self.columns, a0, a0color);
            let mode = read_2d_mode(&mut self.reader).ok_or(CcittDecodeError::UnexpectedEof)?;

            match mode {
                TwoDMode::VerticalRight(delta) | TwoDMode::VerticalLeft(delta) => {
                    let a1 = if let TwoDMode::VerticalRight(delta) = mode {
                        // Overflow → treat as end-of-row (best-effort).
                        b1.checked_add(delta).unwrap_or(self.columns)
                    } else {
                        // Underflow → clamp to 0 (best-effort).
                        b1.saturating_sub(delta)
                    };
                    if !a0color {
                        fill_bits(&mut self.row_buf, self.columns, a0.unwrap_or(0), a1);
                    }
                    if a1 >= self.columns {
                        return Ok(());
                    }
                    if a0.is_some_and(|pos| pos >= a1) {
                        return Ok(());
                    }
                    a0 = Some(a1);
                    a0color = !a0color;
                }
                TwoDMode::Horizontal => {
                    let run_len1 = decode_run_seq(&mut self.reader, a0color)?;
                    let (a0_fill, a1) = match a0 {
                        None => (0, run_len1.saturating_add(1)),
                        Some(pos) => (pos, pos.saturating_add(run_len1)),
                    };
                    if !a0color {
                        fill_bits(&mut self.row_buf, self.columns, a0_fill, a1);
                    }

                    let run_len2 = decode_run_seq(&mut self.reader, !a0color)?;
                    let a2 = a1.saturating_add(run_len2);
                    if a0color {
                        fill_bits(&mut self.row_buf, self.columns, a1, a2);
                    }

                    a0 = Some(a2);
                    if a2 >= self.columns {
                        return Ok(());
                    }
                }
                TwoDMode::Pass => {
                    if !a0color {
                        fill_bits(&mut self.row_buf, self.columns, a0.unwrap_or(0), b2);
                    }
                    if b2 >= self.columns {
                        return Ok(());
                    }
                    a0 = Some(b2);
                }

                TwoDMode::EndOfBlock => {
                    // First EOL of EOFB; bits already consumed in read_2d_mode.
                }

                TwoDMode::Extension => {
                    return Err(CcittDecodeError::UnknownExtensionCode);
                }
            }
        }
    }
}

/// Decode a CCITTFaxDecode-compressed byte stream.
///
/// Supports Group 3 1D (`K = 0`), Group 3 2D (`K > 0`), and Group 4 /
/// MMR (`K < 0`) encoding modes.
///
/// # Errors
///
/// Returns [`CcittDecodeError::ZeroColumns`] if `params.columns` is zero.
/// Truncated or damaged streams are decoded as-is (matching PDFium behaviour).
/// Use `From<CcittDecodeError>` to convert the error into [`ObjectError`] when
/// needed by higher-level callers.
pub fn decode(data: &[u8], params: &CCITTFaxParams) -> Result<Vec<u8>, CcittDecodeError> {
    let rows = usize::try_from(params.rows).unwrap_or(0);
    let mut decoder = CcittDecoder::new(data, params)?;
    decoder.decode_all(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_bits_full_byte() {
        let mut row = [0xffu8; 1];
        fill_bits(&mut row, 8, 0, 8);
        assert_eq!(row[0], 0x00);
    }

    #[test]
    fn fill_bits_single_pixel_msb() {
        let mut row = [0xffu8; 1];
        fill_bits(&mut row, 8, 0, 1);
        assert_eq!(row[0], 0x7f); // top bit cleared
    }

    #[test]
    fn fill_bits_single_pixel_lsb() {
        let mut row = [0xffu8; 1];
        fill_bits(&mut row, 8, 7, 8);
        assert_eq!(row[0], 0xfe); // bottom bit cleared
    }

    #[test]
    fn fill_bits_cross_byte_boundary() {
        let mut row = [0xffu8; 2];
        fill_bits(&mut row, 16, 4, 12); // bits 4-11
        assert_eq!(row[0], 0xf0); // lower nibble cleared
        assert_eq!(row[1], 0x0f); // upper nibble cleared
    }

    #[test]
    fn fill_bits_noop_when_start_equals_end() {
        let mut row = [0xffu8; 1];
        fill_bits(&mut row, 8, 4, 4);
        assert_eq!(row[0], 0xff); // unchanged
    }

    #[test]
    fn fill_bits_clamps_to_columns() {
        let mut row = [0xffu8; 2];
        fill_bits(&mut row, 8, 6, 16); // end clamped from 16 to 8
        // bits 6-7 of byte 0 cleared; byte 1 untouched
        assert_eq!(row[0], 0xfc);
        assert_eq!(row[1], 0xff);
    }

    #[test]
    fn find_bit_all_white() {
        let row = [0xffu8; 2];
        assert_eq!(find_bit(&row, 16, 0, true), 0);
        assert_eq!(find_bit(&row, 16, 0, false), 16); // no black
    }

    #[test]
    fn find_bit_all_black() {
        let row = [0x00u8; 2];
        assert_eq!(find_bit(&row, 16, 0, false), 0);
        assert_eq!(find_bit(&row, 16, 0, true), 16); // no white
    }

    #[test]
    fn find_bit_mixed() {
        let row = [0b1111_0000u8]; // white in bits 0-3, black in 4-7
        assert_eq!(find_bit(&row, 8, 0, false), 4); // first black at 4
        assert_eq!(find_bit(&row, 8, 4, true), 8); // no white after 4 → max
    }

    #[test]
    fn find_bit_respects_start_pos() {
        let row = [0b0000_0001u8]; // only bit 7 is white
        assert_eq!(find_bit(&row, 8, 0, true), 7);
        assert_eq!(find_bit(&row, 8, 8, true), 8); // start >= max → max
    }

    // ── find_b1_b2 ───────────────────────────────────────────────────────────

    #[test]
    fn find_b1_b2_all_white_ref() {
        let ref_row = [0xffu8; 1]; // all white
        // a0=None (before first pixel), white coding, ref all white → no opposite-color transition
        let (b1, b2) = find_b1_b2(&ref_row, 8, None, true);
        assert_eq!((b1, b2), (8, 8)); // not found
    }

    #[test]
    fn find_b1_b2_transition_at_midpoint() {
        let ref_row = [0b1111_0000u8]; // white 0-3, black 4-7
        // a0=None coding=white: first opposite (black) in ref at 4 → b1=4
        // then next same-color (black) after b1 → no white → b2=8
        let (b1, b2) = find_b1_b2(&ref_row, 8, None, true);
        assert_eq!(b1, 4);
        assert_eq!(b2, 8);
    }

    // ── Test helpers ─────────────────────────────────────────────────────────

    fn params_1d(columns: usize, rows: usize) -> CCITTFaxParams {
        CCITTFaxParams {
            k: 0,
            columns,
            rows,
            ..Default::default()
        }
    }

    fn params_g4(columns: usize, rows: usize) -> CCITTFaxParams {
        CCITTFaxParams {
            k: -1,
            columns,
            rows,
            ..Default::default()
        }
    }

    fn params_g3_2d(columns: usize, rows: usize) -> CCITTFaxParams {
        CCITTFaxParams {
            k: 1,
            columns,
            rows,
            ..Default::default()
        }
    }

    // ── get_run / decode_run_seq ──────────────────────────────────────────────

    #[test]
    fn decode_run_seq_white_run_8() {
        // White run-8 = 5-bit code 10011 packed MSB-first → byte 0x98.
        let data = [0x98u8];
        let mut r = BitReader::new(&data);
        let run = decode_run_seq(&mut r, true);
        assert_eq!(run, Ok(8));
    }

    #[test]
    fn decode_run_seq_black_run_2() {
        // Black run-2 is at level 2 in BLACK_RUN_INS: code 0x03 (binary 11, 2 bits).
        // Packed MSB-first: 1100_0000 = 0xC0.
        let data = [0xC0u8];
        let mut r = BitReader::new(&data);
        let run = decode_run_seq(&mut r, false);
        assert_eq!(run, Ok(2));
    }

    #[test]
    fn decode_run_seq_truncated_returns_err() {
        let data: &[u8] = &[];
        let mut r = BitReader::new(data);
        assert_eq!(
            decode_run_seq(&mut r, true),
            Err(CcittDecodeError::InvalidCode)
        );
    }

    // ── skip_eol ─────────────────────────────────────────────────────────────

    #[test]
    fn skip_eol_present_12_bits() {
        // 12-bit EOL = 000000000001 padded to 2 bytes: 0x00 0x10
        let data = [0x00u8, 0x10u8];
        let mut r = BitReader::new(&data);
        skip_eol(&mut r);
        assert_eq!(r.pos(), 12);
    }

    #[test]
    fn skip_eol_absent_resets_position() {
        // Starts with a 1-bit, which is fewer than 11 zeros → not an EOL.
        let data = [0x80u8]; // 1000_0000
        let mut r = BitReader::new(&data);
        skip_eol(&mut r);
        assert_eq!(r.pos(), 0); // reset: was only 1 bit consumed
    }

    // ── read_2d_mode ─────────────────────────────────────────────────────────

    #[allow(clippy::arithmetic_side_effects)]
    fn bits_to_bytes(bits: &[u8]) -> Vec<u8> {
        let nbytes = bits.len().div_ceil(8);
        let mut out = vec![0u8; nbytes];
        for (i, &b) in bits.iter().enumerate() {
            if b != 0 {
                let byte_i = i / 8;
                let bit_i = 7 - (i % 8);
                if let Some(slot) = out.get_mut(byte_i) {
                    *slot |= 1 << bit_i;
                }
            }
        }
        out
    }

    #[test]
    fn read_2d_mode_vertical_0() {
        let data = bits_to_bytes(&[1]);
        let mut r = BitReader::new(&data);
        assert_eq!(read_2d_mode(&mut r), Some(TwoDMode::VerticalRight(0)));
        assert_eq!(r.pos(), 1);
    }

    #[test]
    fn read_2d_mode_vertical_plus1() {
        let data = bits_to_bytes(&[0, 1, 1]);
        let mut r = BitReader::new(&data);
        assert_eq!(read_2d_mode(&mut r), Some(TwoDMode::VerticalRight(1)));
        assert_eq!(r.pos(), 3);
    }

    #[test]
    fn read_2d_mode_vertical_minus1() {
        let data = bits_to_bytes(&[0, 1, 0]);
        let mut r = BitReader::new(&data);
        assert_eq!(read_2d_mode(&mut r), Some(TwoDMode::VerticalLeft(1)));
    }

    #[test]
    fn read_2d_mode_horizontal() {
        let data = bits_to_bytes(&[0, 0, 1, 0, 0, 0, 0, 0]); // 001 + padding
        let mut r = BitReader::new(&data);
        assert_eq!(read_2d_mode(&mut r), Some(TwoDMode::Horizontal));
        assert_eq!(r.pos(), 3);
    }

    #[test]
    fn read_2d_mode_pass() {
        let data = bits_to_bytes(&[0, 0, 0, 1, 0, 0, 0, 0]); // 0001 + padding
        let mut r = BitReader::new(&data);
        assert_eq!(read_2d_mode(&mut r), Some(TwoDMode::Pass));
        assert_eq!(r.pos(), 4);
    }

    #[test]
    fn read_2d_mode_vertical_plus2() {
        // 000011 1 → V(+2), code is 6 bits + 1 for direction = 7 bits total
        let data = bits_to_bytes(&[0, 0, 0, 0, 1, 1, 1, 0]);
        let mut r = BitReader::new(&data);
        assert_eq!(read_2d_mode(&mut r), Some(TwoDMode::VerticalRight(2)));
        assert_eq!(r.pos(), 6);
    }

    #[test]
    fn read_2d_mode_vertical_minus2() {
        // Codeword 000010: bit4=1, bit5=0 → V(-2)
        let data = bits_to_bytes(&[0, 0, 0, 0, 1, 0, 0, 0]);
        let mut r = BitReader::new(&data);
        assert_eq!(read_2d_mode(&mut r), Some(TwoDMode::VerticalLeft(2)));
    }

    #[test]
    fn read_2d_mode_vertical_plus3() {
        // Codeword 0000011: bit4=0, bit5=1, bit6=1 → V(+3)
        let data = bits_to_bytes(&[0, 0, 0, 0, 0, 1, 1, 0]);
        let mut r = BitReader::new(&data);
        assert_eq!(read_2d_mode(&mut r), Some(TwoDMode::VerticalRight(3)));
        assert_eq!(r.pos(), 7);
    }

    #[test]
    fn read_2d_mode_end_of_block() {
        // 0000001 + 3 skipped = 10 bits consumed total
        let data = bits_to_bytes(&[0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        let mut r = BitReader::new(&data);
        assert_eq!(read_2d_mode(&mut r), Some(TwoDMode::EndOfBlock));
        assert_eq!(r.pos(), 10);
    }

    #[test]
    fn read_2d_mode_extension() {
        // 0000000 + 5 skipped = 12 bits consumed total
        let data = bits_to_bytes(&[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        let mut r = BitReader::new(&data);
        assert_eq!(read_2d_mode(&mut r), Some(TwoDMode::Extension));
        assert_eq!(r.pos(), 12);
    }

    #[test]
    fn read_2d_mode_truncated_returns_none() {
        let data: &[u8] = &[];
        let mut r = BitReader::new(data);
        assert_eq!(read_2d_mode(&mut r), None);
    }

    // ── Group 3 1D ───────────────────────────────────────────────────────────

    // Build a 1D CCITT stream that encodes a single all-white row of `columns`
    // pixels.  The stream is a raw sequence of bits with no EOL.
    //
    // For columns = 8: white terminating code for run-8 is 5-bit 10011 (0x13
    // accumulated over 5 levels in WHITE_RUN_INS).
    // Byte stream: 10011_000 = 0x98.
    #[test]
    fn decode_1d_all_white_8px() {
        // White run-8 code: 5 bits → 10011 → pad to byte → 0x98
        let data = [0x98u8];
        let out = decode(&data, &params_1d(8, 1)).expect("decode failed");
        assert_eq!(out, [0xff]); // all white
    }

    // White run-1 code is a 6-bit code 000111 (0x07 accumulated after 6 levels).
    // Bits: 000111_00 → byte 0x1c.
    #[test]
    fn decode_1d_all_white_1px() {
        let data = [0x1cu8];
        let out = decode(&data, &params_1d(1, 1)).expect("decode failed");
        assert_eq!(out, [0xff]);
    }

    // T.4 always starts with a white run (possibly of length 0).
    // White run-0 = 8-bit code 00110101 (0x35 accumulated at level 8).
    // Black run-1 = 3-bit code 010 (0x02 accumulated at level 3).
    // Stream: [0x35, 0x40] → white-0 (8 bits) + black-1 (3 bits) + padding.
    #[test]
    fn decode_1d_all_black_1px() {
        let data = [0x35u8, 0x40u8];
        let out = decode(&data, &params_1d(1, 1)).expect("decode failed");
        // Row starts 0xFF; fill_bits clears MSB (pixel 0) → 0x7F.
        assert_eq!(out, [0x7f]);
    }

    // ── black_is1 inversion ──────────────────────────────────────────────────

    #[test]
    fn decode_1d_black_is1_inverts_output() {
        let data = [0x98u8]; // all-white row (8 px, K=0)
        let mut p = params_1d(8, 1);
        p.black_is1 = true;
        let out = decode(&data, &p).expect("decode failed");
        assert_eq!(out, [0x00]); // inverted: was 0xFF → 0x00
    }

    // ── Group 3 2D ───────────────────────────────────────────────────────────

    #[test]
    fn decode_g3_2d_all_white_row() {
        // Group 3 2D: tag bit=1 selects 1D encoding for the first row.
        // Tag bit (1) + white run-8 code (10011) = 6 bits → 1_10011_00 = 0xCC
        let data = [0xccu8];
        let out = decode(&data, &params_g3_2d(8, 1)).expect("decode failed");
        assert_eq!(out, [0xff]);
    }

    // ── Group 4 ──────────────────────────────────────────────────────────────

    #[test]
    fn decode_g4_empty_gives_empty() {
        let data: &[u8] = &[];
        let out = decode(data, &params_g4(8, 1)).expect("decode failed");
        // No bits → skip_eol reads nothing → exhausted → no rows appended.
        assert_eq!(out.len(), 0);
    }

    // ── encoded_byte_align ───────────────────────────────────────────────────

    #[test]
    fn decode_byte_align_skips_zero_padding() {
        // White run-8 = 5 bits (10011); with byte-align the 3 trailing zeros
        // are skipped, and the second row should start on a byte boundary.
        let data = [0x98u8, 0x98u8]; // two rows of all-white 8px, each 5 bits + 3 pad
        let mut p = params_1d(8, 2);
        p.encoded_byte_align = true;
        let out = decode(&data, &p).expect("decode failed");
        assert_eq!(out, [0xff, 0xff]); // two all-white rows
    }

    // ── Error cases ──────────────────────────────────────────────────────────

    #[test]
    fn decode_zero_columns_returns_error() {
        let mut p = params_1d(0, 1);
        p.columns = 0;
        assert!(decode(&[], &p).is_err());
    }

    #[test]
    fn decode_zero_columns_error_is_zero_columns_variant() {
        let mut p = params_1d(0, 1);
        p.columns = 0;
        let err = decode(&[], &p).unwrap_err();
        assert!(matches!(err, CcittDecodeError::ZeroColumns));
    }

    #[test]
    fn decode_truncated_stream_returns_partial() {
        // 4 columns, 2 rows requested, but only 1 byte of data.
        let data = [0xf0u8]; // some bits
        let p = params_1d(4, 2);
        // Should not panic; returns whatever was decoded.
        let _ = decode(&data, &p).expect("decode should not error on truncation");
    }

    // ── Edge-case tests ──────────────────────────────────────────────────────

    #[test]
    fn decode_1d_multi_row_all_white() {
        // Two all-white rows of 8 pixels each.
        // White run-8 = 5 bits (10011). Two consecutive rows: 10011_10011_...
        // First row: bits 0-4 = 10011, second row: bits 5-9 = 10011.
        // Byte 0: 10011_100 = 0x9C, Byte 1: 11_000000 = 0xC0
        let data = [0x9cu8, 0xc0u8];
        let out = decode(&data, &params_1d(8, 2)).expect("decode failed");
        assert_eq!(out, [0xff, 0xff]); // two all-white rows
    }

    #[test]
    fn fill_bits_start_beyond_columns_is_noop() {
        let mut row = [0xffu8; 2];
        fill_bits(&mut row, 8, 10, 16); // start > columns → no-op
        assert_eq!(row, [0xff, 0xff]);
    }

    #[test]
    fn find_bit_large_row_uses_skip_path() {
        // Create a row > 8 bytes so the 8-byte skip loop is exercised.
        let mut row = vec![0xffu8; 16]; // all white (128 pixels)
        // Place a single black pixel at position 100: byte 12, bit 4
        if let Some(b) = row.get_mut(12) {
            *b = 0xf7; // clear bit at position 100 (12*8 + 4 = 100, bit 4 = 0b1111_0111)
        }
        assert_eq!(find_bit(&row, 128, 0, false), 100);
    }

    #[test]
    fn decode_1d_black_is1_with_g4() {
        // Group 4 with empty data and black_is1.
        let data: &[u8] = &[];
        let mut p = params_g4(8, 1);
        p.black_is1 = true;
        let out = decode(data, &p).expect("decode failed");
        assert_eq!(out.len(), 0); // no rows decoded from empty data
    }

    #[test]
    fn ref_pixel_out_of_bounds_returns_white() {
        let row = [0x00u8]; // all black
        // Position beyond the row should return white.
        assert!(ref_pixel(&row, 8));
        assert!(ref_pixel(&row, 100));
    }

    #[test]
    fn ref_pixel_reads_correct_bits() {
        let row = [0b1010_0101u8];
        assert!(ref_pixel(&row, 0)); // bit 0 = 1 (white)
        assert!(!ref_pixel(&row, 1)); // bit 1 = 0 (black)
        assert!(ref_pixel(&row, 2)); // bit 2 = 1 (white)
        assert!(!ref_pixel(&row, 3)); // bit 3 = 0 (black)
    }

    #[test]
    fn one_lead_pos_all_zero() {
        assert_eq!(one_lead_pos(0), 8);
    }

    #[test]
    fn one_lead_pos_msb_set() {
        assert_eq!(one_lead_pos(0x80), 0);
        assert_eq!(one_lead_pos(0xff), 0);
    }

    #[test]
    fn one_lead_pos_lsb_only() {
        assert_eq!(one_lead_pos(0x01), 7);
    }

    #[test]
    fn ccitt_decode_error_converts_to_object_error() {
        let ccitt_err = CcittDecodeError::ZeroColumns;
        let obj_err: ObjectError = ccitt_err.into();
        assert!(matches!(obj_err, ObjectError::DecompressionError(_)));
    }

    // ── Lenient decoding / damaged-row tolerance ─────────────────────────────

    #[test]
    fn decode_g4_regression_is_lenient() {
        // Two consecutive V(-1) codes on an all-white reference row cause
        // a monotonicity regression (a0=7 >= a1=7).  The decoder should
        // end the row early instead of returning an error.
        //
        // V(-1) = 010 (3 bits).  Two in a row: 010_010_00 → 0x48.
        let data = [0x48u8];
        let out = decode(&data, &params_g4(8, 1)).expect("regression should be lenient");
        assert_eq!(out, [0xff]); // partial all-white row
    }

    #[test]
    fn decode_g4_extension_code_tolerated() {
        // V(-1) moves a0 to 7, then an Extension code (0000000 + 5 padding
        // bits) triggers UnknownExtensionCode.  With default
        // damaged_rows_before_error = 0 (unlimited), the partial row is
        // returned instead of an error.
        //
        // V(-1) = 010, Extension = 0000000 + 5 bits = 15 bits total.
        // Bits: 010_0000000_00000_0 → [0x40, 0x00].
        let data = [0x40u8, 0x00u8];
        let out = decode(&data, &params_g4(8, 1)).expect("extension should be tolerated");
        assert_eq!(out, [0xff]); // partial all-white row
    }

    #[test]
    fn decode_g4_damaged_rows_exceeds_limit() {
        // Two rows both triggering UnknownExtensionCode.
        // V(-1) = 010, Extension = 0000000 + 5 padding = 15 bits per row.
        // With byte alignment, each row pads to 16 bits (2 bytes).
        let data = [0x40u8, 0x00, 0x40, 0x00];
        let mut p = params_g4(8, 2);
        p.encoded_byte_align = true;
        p.damaged_rows_before_error = 1;
        let err = decode(&data, &p).unwrap_err();
        assert!(matches!(err, CcittDecodeError::UnknownExtensionCode));
    }
}
