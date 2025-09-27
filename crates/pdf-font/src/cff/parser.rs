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

pub fn parse_int(cursor: &mut Cursor, b0: u8) -> Result<i32, CursorReadError> {
    // Size   b0 range     Value range              Value calculation
    //--------------------------------------------------------------------------------
    // 1      32 to 246    -107 to +107             b0 - 139
    // 2      247 to 250   +108 to +1131            (b0 - 247) * 256 + b1 + 108
    // 2      251 to 254   -1131 to -108            -(b0 - 251) * 256 - b1 - 108
    // 3      28           -32768 to +32767         b1 << 8 | b2
    // 5      29           -(2^31) to +(2^31 - 1)   b1 << 24 | b2 << 16 | b3 << 8 | b4
    // <https://learn.microsoft.com/en-us/typography/opentype/spec/cff2#table-3-operand-encoding>
    Ok(match b0 {
        32..=246 => b0 as i32 - 139,
        247..=250 => (b0 as i32 - 247) * 256 + cursor.read_u8()? as i32 + 108,
        251..=254 => -(b0 as i32 - 251) * 256 - cursor.read_u8()? as i32 - 108,
        28 => cursor.read_u16()? as i32,
        29 => {
            let b1 = cursor.read_u8()? as u32;
            let b2 = cursor.read_u8()? as u32;
            let b3 = cursor.read_u8()? as u32;
            let b4 = cursor.read_u8()? as u32;
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
        // windows(2) always yields 2-length slices
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

/// CFF DICT token: either a number or an operator (op).
#[derive(Debug, Clone)]
pub(crate) enum DictToken {
    Number(i32),
    Operator(u16),
}

pub(crate) fn parse_dict(data: &[u8]) -> Result<Vec<DictToken>, CompactFontFormatError> {
    let mut cur = Cursor::new(data);
    let mut out = Vec::new();

    while cur.pos() < cur.len() {
        let b = cur.read_u8()?;
        let token = match b {
            0..=21 => {
                // Operator byte
                if b == 12 {
                    let b2 = cur.read_u8()?;
                    DictToken::Operator(((u16::from(b)) << 8) | u16::from(b2))
                } else {
                    DictToken::Operator(u16::from(b))
                }
            }
            28 | 32..=254 => {
                let v = parse_int(&mut cur, b)?;
                DictToken::Number(v)
            }

            29 => {
                // long int (4 bytes, big endian, signed)
                let s = cur.read_n(4)?;
                let val = i32::from_be_bytes([s[0], s[1], s[2], s[3]]);
                DictToken::Number(val)
            }
            30 => {
                return Err(CompactFontFormatError::UnsupportedRealNumber);
            }
            _ => return Err(CompactFontFormatError::UnexpectedDictByte(b)),
        };
        out.push(token);
    }
    Ok(out)
}
