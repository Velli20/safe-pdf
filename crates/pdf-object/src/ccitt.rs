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

use crate::{dictionary::Dictionary, error::ObjectError, object_variant::ObjectVariant};

// ─── Decode parameters ───────────────────────────────────────────────────────

/// Decode parameters for the `CCITTFaxDecode` filter (PDF spec §7.4.6, Table 11).
#[derive(Debug, Clone)]
pub struct CCITTFaxParams {
    /// Selects the encoding scheme.
    /// `K < 0` = Group 4 (T.6 MMR); `K = 0` = Group 3 1D; `K > 0` = Group 3 2D.
    /// Default: `0`.
    pub k: i32,
    /// Width of the image in pixels. Default: `1728`.
    pub columns: u32,
    /// Number of rows. `0` means decode until end-of-block / data exhaustion. Default: `0`.
    pub rows: u32,
    /// Whether EOL bit patterns appear before each row. Default: `false`.
    pub end_of_line: bool,
    /// Whether each EOL code begins on a byte boundary. Default: `false`.
    pub encoded_byte_align: bool,
    /// Whether a block terminator (EOFB / RTC) is present. Default: `true`.
    pub end_of_block: bool,
    /// If `true`, black = 1 and white = 0. Default: `false` (white = 1).
    pub black_is1: bool,
    /// Tolerated number of damaged rows before returning an error. Default: `0`.
    pub damaged_rows_before_error: u32,
}

impl Default for CCITTFaxParams {
    fn default() -> Self {
        Self {
            k: 0,
            columns: 1728,
            rows: 0,
            end_of_line: false,
            encoded_byte_align: false,
            end_of_block: true,
            black_is1: false,
            damaged_rows_before_error: 0,
        }
    }
}

impl CCITTFaxParams {
    /// Parse decode parameters from a PDF `/DecodeParms` dictionary.
    ///
    /// Values are extracted by directly matching [`ObjectVariant`] variants in the
    /// provided [`Dictionary`]. This function does not resolve indirect references
    /// on its own; if the `/DecodeParms` entry is an indirect reference (as allowed
    /// by the PDF specification), the caller must resolve it to a dictionary before
    /// calling this method.
    pub fn from_dictionary(dict: &Dictionary) -> Self {
        let mut p = Self::default();

        if let Some(ObjectVariant::Integer(v)) = dict.get("K") {
            p.k = i32::try_from(*v).unwrap_or_default();
        }
        if let Some(ObjectVariant::Integer(v)) = dict.get("Columns")
            && *v > 0
        {
            p.columns = u32::try_from(*v).unwrap_or(p.columns);
        }
        if let Some(ObjectVariant::Integer(v)) = dict.get("Rows")
            && *v >= 0
        {
            p.rows = u32::try_from(*v).unwrap_or_default();
        }
        if let Some(ObjectVariant::Boolean(v)) = dict.get("EndOfLine") {
            p.end_of_line = *v;
        }
        if let Some(ObjectVariant::Boolean(v)) = dict.get("EncodedByteAlign") {
            p.encoded_byte_align = *v;
        }
        if let Some(ObjectVariant::Boolean(v)) = dict.get("EndOfBlock") {
            p.end_of_block = *v;
        }
        if let Some(ObjectVariant::Boolean(v)) = dict.get("BlackIs1") {
            p.black_is1 = *v;
        }
        if let Some(ObjectVariant::Integer(v)) = dict.get("DamagedRowsBeforeError")
            && *v >= 0
        {
            p.damaged_rows_before_error = u32::try_from(*v).unwrap_or_default();
        }

        p
    }
}

// ─── Huffman tables ──────────────────────────────────────────────────────────

/// For byte value `v`, `ONE_LEAD_POS[v]` is the MSB-first bit offset (0 = MSB)
/// of the first set bit.  Value `8` means no set bit (`v == 0`).
const ONE_LEAD_POS: [u8; 256] = [
    8, 7, 6, 6, 5, 5, 5, 5, 4, 4, 4, 4, 4, 4, 4, 4, // 0-15
    3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, // 16-31
    2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, // 32-47
    2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, // 48-63
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, // 64-79
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, // 80-95
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, // 96-111
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, // 112-127
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // 128-143
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // 144-159
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // 160-175
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // 176-191
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // 192-207
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // 208-223
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // 224-239
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // 240-255
];

/// Black-run Huffman instruction table (ITU-T T.4 Table 1 / T.6).
///
/// Format: each level starts with a `count` byte (number of entries at this
/// depth), followed by `count × 3` bytes of `(code, run_lo, run_hi)` tuples
/// where `run = run_lo + run_hi * 256`.  `0xFF` terminates the table.
const BLACK_RUN_INS: &[u8] = &[
    // level 1 – 0 entries
    0, // level 2 – 2 entries
    2, 0x02, 3, 0, 0x03, 2, 0, // level 3 – 2 entries
    2, 0x02, 1, 0, 0x03, 4, 0, // level 4 – 2 entries
    2, 0x02, 6, 0, 0x03, 5, 0, // level 5 – 1 entry
    1, 0x03, 7, 0, // level 6 – 2 entries
    2, 0x04, 9, 0, 0x05, 8, 0, // level 7 – 3 entries
    3, 0x04, 10, 0, 0x05, 11, 0, 0x07, 12, 0, // level 8 – 2 entries
    2, 0x04, 13, 0, 0x07, 14, 0, // level 9 – 1 entry
    1, 0x18, 15, 0, // level 10 – 5 entries  (includes makeup-64 and terminators 0,16-18)
    5, 0x08, 18, 0, 0x0f, 64, 0, 0x17, 16, 0, 0x18, 17, 0, 0x37, 0, 0,
    // level 11 – 10 entries  (makeups 1792-1920 and terminators 19-25)
    10, 0x08, 0, 7, 0x0c, 64, 7, 0x0d, 128, 7, // runs 1792, 1856, 1920
    0x17, 24, 0, 0x18, 25, 0, 0x28, 23, 0, 0x37, 22, 0, 0x67, 19, 0, 0x68, 20, 0, 0x6c, 21, 0,
    // level 12 – 54 entries  (makeups 1984-2560 and many terminators)
    54, 0x12, 192, 7, 0x13, 0, 8, 0x14, 64, 8, 0x15, 128, 8, // 1984-2176
    0x16, 192, 8, 0x17, 0, 9, 0x1c, 64, 9, 0x1d, 128, 9, // 2240-2432
    0x1e, 192, 9, 0x1f, 0, 10, // 2496, 2560
    0x24, 52, 0, 0x27, 55, 0, 0x28, 56, 0, 0x2b, 59, 0, 0x2c, 60, 0, 0x33, 64, 1, 0x34, 128, 1,
    0x35, 192, 1, // 320, 384, 448
    0x37, 53, 0, 0x38, 54, 0, 0x52, 50, 0, 0x53, 51, 0, 0x54, 44, 0, 0x55, 45, 0, 0x56, 46, 0,
    0x57, 47, 0, 0x58, 57, 0, 0x59, 58, 0, 0x5a, 61, 0, 0x5b, 0, 1, // 256
    0x64, 48, 0, 0x65, 49, 0, 0x66, 62, 0, 0x67, 63, 0, 0x68, 30, 0, 0x69, 31, 0, 0x6a, 32, 0,
    0x6b, 33, 0, 0x6c, 40, 0, 0x6d, 41, 0, 0xc8, 128, 0, 0xc9, 192, 0, 0xca, 26, 0, 0xcb, 27, 0,
    0xcc, 28, 0, 0xcd, 29, 0, 0xd2, 34, 0, 0xd3, 35, 0, 0xd4, 36, 0, 0xd5, 37, 0, 0xd6, 38, 0,
    0xd7, 39, 0, 0xda, 42, 0, 0xdb, 43, 0,
    // level 13 – 20 entries  (makeups 512-1728 in stride-64 groups)
    20, 0x4a, 128, 2, 0x4b, 192, 2, 0x4c, 0, 3, 0x4d, 64, 3, // 640-832
    0x52, 0, 5, 0x53, 64, 5, 0x54, 128, 5, 0x55, 192, 5, // 1280-1472
    0x5a, 0, 6, 0x5b, 64, 6, 0x64, 128, 6, 0x65, 192, 6, // 1536-1728
    0x6c, 0, 2, 0x6d, 64, 2, // 512, 576
    0x72, 128, 3, 0x73, 192, 3, 0x74, 0, 4, 0x75, 64, 4, // 896-1088
    0x76, 128, 4, 0x77, 192, 4, // 1152, 1216
    // sentinel
    0xff,
];

/// White-run Huffman instruction table (ITU-T T.4 Table 2 / T.6).
const WHITE_RUN_INS: &[u8] = &[
    // levels 1-3 – 0 entries each
    0, 0, 0, // level 4 – 6 entries  (terminators 2-7)
    6, 0x07, 2, 0, 0x08, 3, 0, 0x0b, 4, 0, 0x0c, 5, 0, 0x0e, 6, 0, 0x0f, 7, 0,
    // level 5 – 6 entries  (terminators 8-11 and makeups 64, 128)
    6, 0x07, 10, 0, 0x08, 11, 0, 0x12, 128, 0, 0x13, 8, 0, 0x14, 9, 0, 0x1b, 64, 0,
    // level 6 – 9 entries  (terminators 1,12-17, makeup 192, makeup 1664)
    9, 0x03, 13, 0, 0x07, 1, 0, 0x08, 12, 0, 0x17, 192, 0, 0x18, 128, 6, // 1664
    0x2a, 16, 0, 0x2b, 17, 0, 0x34, 14, 0, 0x35, 15, 0,
    // level 7 – 12 entries  (terminators 18-28 and makeup 256)
    12, 0x03, 22, 0, 0x04, 23, 0, 0x08, 20, 0, 0x0c, 19, 0, 0x13, 26, 0, 0x17, 21, 0, 0x18, 28, 0,
    0x24, 27, 0, 0x27, 18, 0, 0x28, 24, 0, 0x2b, 25, 0, 0x37, 0, 1, // 256
    // level 8 – 42 entries  (terminators 0,29-63, makeups 320-640)
    42, 0x02, 29, 0, 0x03, 30, 0, 0x04, 45, 0, 0x05, 46, 0, 0x0a, 47, 0, 0x0b, 48, 0, 0x12, 33, 0,
    0x13, 34, 0, 0x14, 35, 0, 0x15, 36, 0, 0x16, 37, 0, 0x17, 38, 0, 0x1a, 31, 0, 0x1b, 32, 0,
    0x24, 53, 0, 0x25, 54, 0, 0x28, 39, 0, 0x29, 40, 0, 0x2a, 41, 0, 0x2b, 42, 0, 0x2c, 43, 0,
    0x2d, 44, 0, 0x32, 61, 0, 0x33, 62, 0, 0x34, 63, 0, 0x35, 0, 0, 0x36, 64, 1, 0x37, 128,
    1, // 320, 384
    0x4a, 59, 0, 0x4b, 60, 0, 0x52, 49, 0, 0x53, 50, 0, 0x54, 51, 0, 0x55, 52, 0, 0x58, 55, 0,
    0x59, 56, 0, 0x5a, 57, 0, 0x5b, 58, 0, 0x64, 192, 1, 0x65, 0, 2, 0x67, 128, 2, 0x68, 64,
    2, // 448,512,640,576
    // level 9 – 16 entries  (makeups 704-1728 except 1664)
    16, 0x98, 192, 5, 0x99, 0, 6, 0x9a, 64, 6, 0x9b, 192, 6, // 1472-1728
    0xcc, 192, 2, 0xcd, 0, 3, // 704, 768
    0xd2, 64, 3, 0xd3, 128, 3, 0xd4, 192, 3, // 832-960
    0xd5, 0, 4, 0xd6, 64, 4, 0xd7, 128, 4, 0xd8, 192, 4, // 1024-1216
    0xd9, 0, 5, 0xda, 64, 5, 0xdb, 128, 5, // 1280-1408
    // level 10 – 0 entries
    0, // level 11 – 3 entries  (makeups 1792-1920)
    3, 0x08, 0, 7, 0x0c, 64, 7, 0x0d, 128, 7, // 1792, 1856, 1920
    // level 12 – 10 entries  (makeups 1984-2560)
    10, 0x12, 192, 7, 0x13, 0, 8, 0x14, 64, 8, 0x15, 128, 8, // 1984-2176
    0x16, 192, 8, 0x17, 0, 9, 0x1c, 64, 9, 0x1d, 128, 9, // 2240-2432
    0x1e, 192, 9, 0x1f, 0, 10, // 2496, 2560
    // sentinel
    0xff,
];

// ─── Bit reader ──────────────────────────────────────────────────────────────

/// Reads bits MSB-first from a byte slice.
struct BitReader<'a> {
    src: &'a [u8],
    bit_pos: usize,
}

impl<'a> BitReader<'a> {
    fn new(src: &'a [u8]) -> Self {
        Self { src, bit_pos: 0 }
    }

    /// Read the next bit and advance. Returns `None` when exhausted.
    #[allow(clippy::arithmetic_side_effects)]
    fn next_bit(&mut self) -> Option<bool> {
        let pos = self.bit_pos;
        let byte = self.src.get(pos / 8)?;
        self.bit_pos += 1;
        Some((byte >> (7 - pos % 8)) & 1 != 0)
    }

    fn pos(&self) -> usize {
        self.bit_pos
    }

    fn set_pos(&mut self, pos: usize) {
        self.bit_pos = pos;
    }

    fn exhausted(&self) -> bool {
        self.bit_pos >= self.src.len().saturating_mul(8)
    }

    /// Advance to the next byte boundary, but only if all padding bits are 0.
    /// Returns `true` if alignment happened, `false` if a non-zero pad bit was
    /// found (the caller should disable byte-alignment for remaining rows).
    #[allow(clippy::arithmetic_side_effects)]
    fn try_align_to_byte(&mut self) -> bool {
        let cur = self.bit_pos;
        let aligned = (cur + 7) & !7;
        for p in cur..aligned {
            let is_set = self
                .src
                .get(p / 8)
                .is_some_and(|&b| (b >> (7 - p % 8)) & 1 != 0);
            if is_set {
                return false;
            }
        }
        self.bit_pos = aligned;
        true
    }
}

// ─── Low-level helpers ───────────────────────────────────────────────────────

/// Returns the MSB-first position of the leading set bit of `byte`
/// (0 = MSB, 7 = LSB).  Returns 8 when `byte == 0`.
#[inline]
fn one_lead_pos(byte: u8) -> usize {
    usize::from(ONE_LEAD_POS.get(usize::from(byte)).copied().unwrap_or(8))
}

/// Returns the color (true = white, false = black) of pixel `pos` in `row`.
/// Out-of-bounds reads return white (imaginary padding).
#[inline]
#[allow(clippy::arithmetic_side_effects)]
fn ref_pixel(row: &[u8], pos: usize) -> bool {
    row.get(pos / 8)
        .is_none_or(|&b| (b >> (7 - pos % 8)) & 1 != 0)
}

/// Set pixels in `[start, end)` to black (0) in `row`.
/// `row` must be at least `(columns + 7) / 8` bytes long.
#[allow(clippy::arithmetic_side_effects)]
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
        for b in middle.iter_mut() {
            *b = 0x00;
        }
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
#[allow(clippy::arithmetic_side_effects)]
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
/// * `a0` – current position on the coding line (−1 = before first pixel).
/// * `a0color` – color of the current coding line at `a0`.
///
/// Returns `(b1, b2)` where each may equal `columns` when not found.
#[allow(clippy::arithmetic_side_effects)]
fn find_b1_b2(ref_row: &[u8], columns: usize, a0: i32, a0color: bool) -> (usize, usize) {
    // Color of the reference line at position a0 (white when a0 < 0).
    let first_bit = if a0 < 0 {
        true
    } else {
        ref_pixel(ref_row, usize::try_from(a0).unwrap_or(0))
    };

    let search_start = usize::try_from(a0.saturating_add(1).max(0)).unwrap_or(0);
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
#[allow(clippy::arithmetic_side_effects)]
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

/// Decode a complete run-length sequence (makeup + terminating codes).
/// Colors: `true` = white, `false` = black.  Returns `None` on stream error.
fn decode_run_seq(reader: &mut BitReader<'_>, color: bool) -> Option<i32> {
    let ins = if color { WHITE_RUN_INS } else { BLACK_RUN_INS };
    let mut total: i32 = 0;
    loop {
        let run = i32::from(get_run(ins, reader)?);
        total = total.saturating_add(run);
        if run < 64 {
            return Some(total);
        }
        // Makeup code: keep reading for the terminating code.
    }
}

/// Skip an end-of-line marker if one is present.
///
/// An EOL consists of ≥ 11 leading zero-bits followed by a one-bit (12 bits
/// total for T.4 EOL).  If fewer than 12 bits are consumed before the
/// one-bit, the position is reset (no EOL was present).
#[allow(clippy::arithmetic_side_effects)]
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

// ─── Row decoders ────────────────────────────────────────────────────────────

/// Decode one Group 3 1D scan line.
///
/// `row` must be pre-filled with `0xFF` (all white).  Black runs are written
/// by calling [`fill_bits`].
fn decode_1d_row(reader: &mut BitReader<'_>, row: &mut [u8], columns: usize) {
    let mut color = true; // start white
    let mut startpos: usize = 0;

    loop {
        if reader.exhausted() {
            return;
        }

        let mut run_len: usize = 0;
        loop {
            let ins = if color { WHITE_RUN_INS } else { BLACK_RUN_INS };
            match get_run(ins, reader) {
                None => {
                    // Stream error: scan forward to the next 1-bit (EOL marker).
                    loop {
                        match reader.next_bit() {
                            None | Some(true) => return,
                            Some(false) => {}
                        }
                    }
                }
                Some(run) => {
                    run_len = run_len.saturating_add(usize::from(run));
                    if run < 64 {
                        break; // terminating code – run complete
                    }
                    // Makeup code – continue reading.
                }
            }
        }

        if !color {
            fill_bits(row, columns, startpos, startpos.saturating_add(run_len));
        }

        startpos = startpos.saturating_add(run_len);
        if startpos >= columns {
            break;
        }
        color = !color;
    }
}

/// Decode one Group 4 (T.6 / MMR) or Group 3 2D scan line.
///
/// `row` must be pre-filled with `0xFF` (all white).
#[allow(clippy::arithmetic_side_effects)]
fn decode_2d_row(reader: &mut BitReader<'_>, row: &mut [u8], ref_row: &[u8], columns: usize) {
    let columns_i32 = i32::try_from(columns).unwrap_or(i32::MAX);
    let mut a0: i32 = -1;
    let mut a0color = true; // white

    'outer: loop {
        if reader.exhausted() {
            return;
        }

        let (b1, b2) = find_b1_b2(ref_row, columns, a0, a0color);

        let v_delta: i32 = {
            let bit0 = match reader.next_bit() {
                Some(b) => b,
                None => return,
            };

            if bit0 {
                // V(0): a1 = b1
                0
            } else {
                let bit1 = match reader.next_bit() {
                    Some(b) => b,
                    None => return,
                };
                let bit2 = match reader.next_bit() {
                    Some(b) => b,
                    None => return,
                };

                if bit1 {
                    // 01x → V(+1) or V(-1)
                    if bit2 { 1 } else { -1 }
                } else if bit2 {
                    // 001 → H mode: two explicit run lengths
                    let mut run_len1 = match decode_run_seq(reader, a0color) {
                        Some(r) => r,
                        None => return,
                    };
                    if a0 < 0 {
                        run_len1 += 1;
                    }
                    let a0s = usize::try_from(a0.max(0)).unwrap_or(0);
                    let a1 = a0.max(0) + run_len1;
                    if !a0color {
                        fill_bits(row, columns, a0s, usize::try_from(a1.max(0)).unwrap_or(0));
                    }

                    let run_len2 = match decode_run_seq(reader, !a0color) {
                        Some(r) => r,
                        None => return,
                    };
                    let a2 = a1 + run_len2;
                    if a0color {
                        fill_bits(
                            row,
                            columns,
                            usize::try_from(a1.max(0)).unwrap_or(0),
                            usize::try_from(a2.max(0)).unwrap_or(0),
                        );
                    }

                    a0 = a2;
                    if a0 < columns_i32 {
                        continue 'outer;
                    }
                    return;
                } else {
                    // 000…
                    let bit3 = match reader.next_bit() {
                        Some(b) => b,
                        None => return,
                    };
                    if bit3 {
                        // 0001 → Pass mode
                        if !a0color {
                            fill_bits(row, columns, usize::try_from(a0.max(0)).unwrap_or(0), b2);
                        }
                        if b2 >= columns {
                            return;
                        }
                        a0 = i32::try_from(b2).unwrap_or(i32::MAX);
                        continue 'outer;
                    }

                    // 0000xx…
                    let nb1 = match reader.next_bit() {
                        Some(b) => b,
                        None => return,
                    };
                    let nb2 = match reader.next_bit() {
                        Some(b) => b,
                        None => return,
                    };

                    if nb1 {
                        // 000011x → V(+2) or V(-2)
                        if nb2 { 2 } else { -2 }
                    } else if nb2 {
                        // 0000010 or 0000011 → V(+3) or V(-3)
                        let nb3 = match reader.next_bit() {
                            Some(b) => b,
                            None => return,
                        };
                        if nb3 { 3 } else { -3 }
                    } else {
                        // 000000x – EOFB marker or extension code
                        let nb3 = match reader.next_bit() {
                            Some(b) => b,
                            None => return,
                        };
                        if nb3 {
                            // First EOL of EOFB; skip 3 remaining bits.
                            for _ in 0..3 {
                                reader.next_bit();
                            }
                            continue 'outer;
                        }
                        // Extension code; skip 5 remaining bits and end row.
                        for _ in 0..5 {
                            reader.next_bit();
                        }
                        return;
                    }
                }
            }
        };

        // Apply V-mode: a1 = b1 + v_delta
        let a1 = i32::try_from(b1).unwrap_or(i32::MAX) + v_delta;
        if !a0color {
            fill_bits(
                row,
                columns,
                usize::try_from(a0.max(0)).unwrap_or(0),
                usize::try_from(a1.max(0)).unwrap_or(0),
            );
        }
        if a1 >= columns_i32 {
            return;
        }
        if a0 >= a1 {
            return; // monotonicity violated – stop
        }
        a0 = a1;
        a0color = !a0color;
    }
}

// ─── Public API ──────────────────────────────────────────────────────────────

/// Decode a CCITTFaxDecode-compressed byte stream.
///
/// Supports Group 3 1D (`K = 0`), Group 3 2D (`K > 0`), and Group 4 /
/// MMR (`K < 0`) encoding modes.
///
/// # Errors
///
/// Returns [`ObjectError::DecompressionError`] if `columns` is zero.
/// Truncated or damaged streams are decoded as-is (matching PDFium behaviour).
#[allow(clippy::arithmetic_side_effects)]
pub fn decode(data: &[u8], params: &CCITTFaxParams) -> Result<Vec<u8>, ObjectError> {
    let columns = usize::try_from(params.columns).unwrap_or(0);
    if columns == 0 {
        return Err(ObjectError::DecompressionError(
            "CCITTFaxDecode: zero column count".into(),
        ));
    }

    let row_bytes = columns.div_ceil(8);
    let rows = usize::try_from(params.rows).unwrap_or(0);

    let mut output: Vec<u8> =
        Vec::with_capacity(rows.saturating_mul(row_bytes).max(row_bytes * 16));

    let mut reader = BitReader::new(data);
    let mut ref_row = vec![0xffu8; row_bytes];
    let mut row_buf = vec![0xffu8; row_bytes];
    let mut decoded_rows: usize = 0;
    let mut byte_align = params.encoded_byte_align;

    loop {
        if rows > 0 && decoded_rows >= rows {
            break;
        }

        // Skip any EOL marker preceding the row (no-op when none present).
        skip_eol(&mut reader);

        if reader.exhausted() {
            break;
        }

        row_buf.fill(0xff);

        match params.k.cmp(&0) {
            std::cmp::Ordering::Less => {
                // Group 4 / MMR (T.6)
                decode_2d_row(&mut reader, &mut row_buf, &ref_row, columns);
                ref_row.copy_from_slice(&row_buf);
            }
            std::cmp::Ordering::Equal => {
                // Group 3, 1D
                decode_1d_row(&mut reader, &mut row_buf, columns);
            }
            std::cmp::Ordering::Greater => {
                // Group 3, 2D: tag bit selects 1D (1) or 2D (0)
                let use_1d = reader.next_bit().unwrap_or(true);
                if use_1d {
                    decode_1d_row(&mut reader, &mut row_buf, columns);
                } else {
                    decode_2d_row(&mut reader, &mut row_buf, &ref_row, columns);
                }
                ref_row.copy_from_slice(&row_buf);
            }
        }

        // Optional trailing EOL.
        if params.end_of_line {
            skip_eol(&mut reader);
        }

        // Optional byte-alignment.  A non-zero padding bit permanently
        // disables alignment for the remainder of the stream.
        if byte_align {
            let cur = reader.pos();
            let aligned = (cur + 7) & !7;
            if cur < aligned && !reader.try_align_to_byte() {
                byte_align = false;
            }
        }

        output.extend_from_slice(&row_buf);
        decoded_rows += 1;
    }

    if params.black_is1 {
        for b in output.iter_mut() {
            *b ^= 0xff;
        }
    }

    Ok(output)
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn params_1d(columns: u32, rows: u32) -> CCITTFaxParams {
        CCITTFaxParams {
            k: 0,
            columns,
            rows,
            end_of_line: false,
            encoded_byte_align: false,
            end_of_block: true,
            black_is1: false,
            damaged_rows_before_error: 0,
        }
    }

    fn params_g4(columns: u32, rows: u32) -> CCITTFaxParams {
        CCITTFaxParams {
            k: -1,
            columns,
            rows,
            end_of_line: false,
            encoded_byte_align: false,
            end_of_block: true,
            black_is1: false,
            damaged_rows_before_error: 0,
        }
    }

    // ── fill_bits ────────────────────────────────────────────────────────────

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

    // ── find_bit ─────────────────────────────────────────────────────────────

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

    // ── Group 4 empty stream ─────────────────────────────────────────────────

    #[test]
    fn decode_g4_empty_gives_empty() {
        let data: &[u8] = &[];
        let out = decode(&data, &params_g4(8, 1)).expect("decode failed");
        // No bits → skip_eol reads nothing → exhausted → no rows appended.
        assert_eq!(out.len(), 0);
    }

    // ── Error cases ──────────────────────────────────────────────────────────

    #[test]
    fn decode_zero_columns_returns_error() {
        let mut p = params_1d(0, 1);
        p.columns = 0;
        assert!(decode(&[], &p).is_err());
    }

    #[test]
    fn decode_truncated_stream_returns_partial() {
        // 4 columns, 2 rows requested, but only 1 byte of data.
        let data = [0xf0u8]; // some bits
        let p = params_1d(4, 2);
        // Should not panic; returns whatever was decoded.
        let _ = decode(&data, &p).expect("decode should not error on truncation");
    }
}
