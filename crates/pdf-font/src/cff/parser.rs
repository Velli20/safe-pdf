use crate::cff::{cursor::Cursor, error::CompactFontFormatError};

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
    Real(String),
    Operator(u16),
}


pub(crate) fn parse_dict(data: &[u8]) -> Result<Vec<DictToken>, CompactFontFormatError> {
    let mut cur = Cursor::new(data);
    let mut out = Vec::new();

    while cur.pos() < cur.len() {
        let b = cur.read_u8()?;
        let token =
            match b {
                0..=21 => {
                    // Operator byte
                    if b == 12 {
                        let b2 = cur.read_u8()?;
                        DictToken::Operator(((u16::from(b)) << 8) | u16::from(b2))
                    } else {
                        DictToken::Operator(u16::from(b))
                    }
                }
                28 => {
                    // short int (2 bytes, big endian, signed)
                    let s = cur.read_n(2)?;
                    let val = i32::from(i16::from_be_bytes([s[0], s[1]]));
                    DictToken::Number(val)
                }
                29 => {
                    // long int (4 bytes, big endian, signed)
                    let s = cur.read_n(4)?;
                    let val = i32::from_be_bytes([s[0], s[1], s[2], s[3]]);
                    DictToken::Number(val)
                }
                30 => {
                    // real number — ASCII nibble encoding. We'll parse to String.
                    fn parse_real(cur: &mut Cursor) -> Result<String, CompactFontFormatError> {
                        let mut chars = String::new();
                        'outer: loop {
                            let nibble_byte = cur.read_u8()?;
                            for &n in &[nibble_byte >> 4, nibble_byte & 0x0f] {
                                match n {
                                    0..=9 => {
                                        let d = b'0'.checked_add(n).ok_or(
                                            CompactFontFormatError::InvalidData(
                                                "digit nibble overflow",
                                            ),
                                        )?;
                                        chars.push(char::from(d));
                                    }
                                    0xA => chars.push('.'),
                                    0xB | 0xC => chars.push('E'),
                                    0xD => chars.push('-'),
                                    0xE => {}            // reserved
                                    0xF => break 'outer, // terminator
                                    _ => {}
                                }
                            }
                        }
                        Ok(chars)
                    }
                    DictToken::Real(parse_real(&mut cur)?)
                }
                32..=246 => {
                    let val = i32::from(b).checked_add(-139).ok_or(
                        CompactFontFormatError::InvalidData("integer overflow in DICT operand"),
                    )?;
                    DictToken::Number(val)
                }
                247..=250 => {
                    let b2 = cur.read_u8()?;
                    let high = (i32::from(b))
                        .checked_sub(247)
                        .and_then(|v| v.checked_mul(256))
                        .ok_or(CompactFontFormatError::InvalidData(
                            "integer overflow in DICT operand",
                        ))?;
                    let val = high
                        .checked_add(i32::from(b2))
                        .and_then(|v| v.checked_add(108))
                        .ok_or(CompactFontFormatError::InvalidData(
                            "integer overflow in DICT operand",
                        ))?;
                    DictToken::Number(val)
                }
                251..=254 => {
                    let b2 = cur.read_u8()?;
                    let high = (i32::from(b))
                        .checked_sub(251)
                        .and_then(|v| v.checked_mul(256))
                        .ok_or(CompactFontFormatError::InvalidData(
                            "integer overflow in DICT operand",
                        ))?;
                    let neg = high
                        .checked_add(i32::from(b2))
                        .and_then(|v| v.checked_add(108))
                        .ok_or(CompactFontFormatError::InvalidData(
                            "integer overflow in DICT operand",
                        ))?;
                    let val = 0i32
                        .checked_sub(neg)
                        .ok_or(CompactFontFormatError::InvalidData(
                            "integer overflow in DICT operand",
                        ))?;
                    DictToken::Number(val)
                }
                _ => return Err(CompactFontFormatError::UnexpectedDictByte(b)),
            };
        out.push(token);
    }
    Ok(out)
}
