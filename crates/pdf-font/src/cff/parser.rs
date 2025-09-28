use crate::cff::{
    cursor::{Cursor, CursorReadError},
    error::CompactFontFormatError,
};

/// Read a big-endian integer from 1..=4 bytes and return it as `usize`.
fn read_be_int(bytes: &[u8]) -> Result<usize, CompactFontFormatError> {
    let v = match bytes {
        [b0] => u32::from(*b0),
        [b0, b1] => u32::from(u16::from_be_bytes([*b0, *b1])),
        [b0, b1, b2] => u32::from_be_bytes([0, *b0, *b1, *b2]),
        [b0, b1, b2, b3] => u32::from_be_bytes([*b0, *b1, *b2, *b3]),
        _ => {
            return Err(CompactFontFormatError::InvalidData(
                "read_be_int: invalid length",
            ));
        }
    };

    usize::try_from(v)
        .map_err(|_| CompactFontFormatError::InvalidData("read_be_int: value out of range"))
}

/// Decode a CFF / CFF2 DICT encoded integer operand.
///
/// The Compact Font Format encodes integer operands in a variable-length form
/// determined by the first byte (`b0`). This function is called after the
/// first byte has already been read. It consumes the remaining bytes (if any)
/// from the provided [`Cursor`] and returns the decoded signed 32‑bit value.
///
/// Encoding summary (see Adobe CFF / OpenType CFF2 spec – operand encoding):
///
/// * `32..=246`  – single byte: value = `b0 as i32 - 139` (range: -107 ..= 107)
/// * `247..=250` – two bytes: value = `(b0 - 247) * 256 + b1 + 108` (108 ..= 1131)
/// * `251..=254` – two bytes: value = `-(b0 - 251) * 256 - b1 - 108` (-1131 ..= -108)
/// * `28`        – three bytes total: next two bytes form a signed big‑endian i16
/// * `29`        – five bytes total: next four bytes form a signed big‑endian i32
///
/// Reference: <https://learn.microsoft.com/en-us/typography/opentype/spec/cff2#table-3-operand-encoding>
///
/// # Parameters
///
/// - `cursor` – cursor positioned immediately after `b0`; additional bytes
///   required by the encoding will be read from it.
/// - `b0` – the first (already read) opcode/operand byte that selects the
///   integer encoding form.
///
/// # Returns
///
/// The decoded signed 32‑bit integer or an `CursorReadError` if there is not enough data
/// to read the remaining bytes of the encoded integer, or if `b0` does not designate a
/// valid integer encoding (in which case `CursorReadError::EndOfData` is
/// returned to signal an unexpected byte for this context).
///
pub fn read_encoded_int(cursor: &mut Cursor, b0: u8) -> Result<i32, CursorReadError> {
    let b0 = i32::from(b0);

    Ok(match b0 {
        32..=246 => b0 - 139,
        247..=250 => (b0 - 247) * 256 + i32::from(cursor.read_u8()?) + 108,
        251..=254 => -(b0 - 251) * 256 - i32::from(cursor.read_u8()?) - 108,
        28 => i32::from(cursor.read_u16()?),
        29 => {
            let b1 = u32::from(cursor.read_u8()?);
            let b2 = u32::from(cursor.read_u8()?);
            let b3 = u32::from(cursor.read_u8()?);
            let b4 = u32::from(cursor.read_u8()?);
            let raw = (b1 << 24) | (b2 << 16) | (b3 << 8) | b4;
            // Convert to signed i32
            if raw & 0x8000_0000 != 0 {
                // Negative number in two's complement
                let v = (!raw).wrapping_add(1);
                -(v as i32)
            } else {
                // Positive number
                raw as i32
            }
        }
        _ => {
            return Err(CursorReadError::EndOfData);
        }
    })
}

/// Parses a CFF INDEX data structure.
///
/// An INDEX is a collection of variable-sized data objects. It consists of a
/// header, an array of 1-based offsets, and a data block containing the objects
/// themselves. The offsets specify the start and end of each object within the
/// data block.
///
/// The structure is as follows:
///
/// 1. `count` (u16): The number of objects in the INDEX. If 0, the INDEX is empty.
/// 2. `offSize` (u8): The size of the offsets in bytes (1 to 4).
/// 3. `offset` (array of `count + 1` entries): The offset array. Each entry is
///    an `offSize`-byte integer. The first offset is always 1.
/// 4. `data` (array of bytes): The object data. The total size of this block is
///    `offset[count] - 1`.
///
/// # Parameters
///
/// - `cur`: A `Cursor` positioned at the start of the INDEX data.
///
/// # Returns
///
/// A `Vec` containing byte slices (`&'a [u8]`) for each object in the INDEX,
/// or a `CompactFontFormatError` if the data is malformed.
pub fn parse_index<'a>(cur: &mut Cursor<'a>) -> Result<Vec<&'a [u8]>, CompactFontFormatError> {
    let count = cur.read_u16()?;
    if count == 0 {
        return Ok(Vec::new());
    }

    // Read offSize (1 byte)
    let off_size = cur.read_u8()?;
    if !(1..=4).contains(&off_size) {
        return Err(CompactFontFormatError::InvalidData("invalid offSize"));
    }

    // Read offsets (count+1 entries, each offSize bytes, 1-based)
    let count = usize::from(count);

    // Compute total bytes for offsets with checked arithmetic
    let offsets_len = count
        .checked_add(1)
        .and_then(|v| v.checked_mul(usize::from(off_size)))
        .ok_or(CompactFontFormatError::InvalidData(
            "index offsets length overflow",
        ))?;

    let offsets_bytes = cur.read_n(offsets_len)?;
    let chunk = usize::from(off_size);
    let offsets: Result<Vec<usize>, _> = offsets_bytes.chunks(chunk).map(read_be_int).collect();
    let offsets = offsets?;

    // Validate offsets
    let first = offsets
        .first()
        .copied()
        .ok_or(CompactFontFormatError::InvalidOffsets)?;
    if first == 0 {
        return Err(CompactFontFormatError::InvalidData("first offset is 0"));
    }
    let last = offsets
        .last()
        .copied()
        .ok_or(CompactFontFormatError::InvalidOffsets)?;

    let block_len = last
        .checked_sub(1)
        .ok_or(CompactFontFormatError::IndexOffsetsOutOfRange)?;
    let block = cur.read_n(block_len)?;

    // Use offsets to slice objects
    let mut objects = Vec::with_capacity(count);

    for pair in offsets.windows(2) {
        let start = pair[0];
        let end = pair[1];
        if start < 1 || end < start {
            return Err(CompactFontFormatError::InvalidOffsets);
        }
        let start = start
            .checked_sub(1)
            .ok_or(CompactFontFormatError::IndexOffsetsOutOfRange)?;
        let end = end
            .checked_sub(1)
            .ok_or(CompactFontFormatError::IndexOffsetsOutOfRange)?;
        if end > block.len() || start > end {
            return Err(CompactFontFormatError::IndexOffsetsOutOfRange);
        }
        objects.push(&block[start..end]);
    }
    Ok(objects)
}
