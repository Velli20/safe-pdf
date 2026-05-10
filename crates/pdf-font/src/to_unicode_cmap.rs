use std::collections::HashMap;

use pdf_parser::cmap::{CMapParser, CMapToken};

use crate::cmap_support::{bytes_to_u32, consume_until_operator, next_cmap_token};

/// A parsed ToUnicode CMap that maps PDF character codes to Unicode scalar values.
///
/// A ToUnicode CMap is embedded as a stream in a font dictionary. It uses a
/// PostScript-like syntax with `beginbfchar`/`endbfchar` and
/// `beginbfrange`/`endbfrange` sections to declare the mapping.
#[derive(Debug)]
pub struct ToUnicodeCMap(HashMap<u16, Vec<char>>);

impl ToUnicodeCMap {
    /// Parse a ToUnicode CMap stream.
    ///
    /// Only the `bfchar` and `bfrange` sections are processed; the rest of the
    /// PostScript-like prologue is ignored. Invalid or unrecognised entries are
    /// silently skipped so that partial CMaps still yield useful results.
    pub fn from_bytes(data: &[u8]) -> Self {
        let mut map: HashMap<u16, Vec<char>> = HashMap::new();
        let mut parser = CMapParser::from(data);

        loop {
            let Ok(token) = next_cmap_token(&mut parser) else {
                break;
            };

            match token {
                Some(CMapToken::Operator(operator)) if operator.as_slice() == b"beginbfchar" => {
                    if !parse_bfchar_section(&mut parser, &mut map) {
                        break;
                    }
                }
                Some(CMapToken::Operator(operator)) if operator.as_slice() == b"beginbfrange" => {
                    if !parse_bfrange_section(&mut parser, &mut map) {
                        break;
                    }
                }
                Some(_) => {}
                None => break,
            }
        }

        Self(map)
    }

    /// Look up the Unicode characters for the given PDF character code.
    ///
    /// Returns `None` if the code has no mapping in this CMap.
    pub fn map_char_code(&self, code: u16) -> Option<&[char]> {
        self.0.get(&code).map(Vec::as_slice)
    }
}

/// Parse a `beginbfchar` ... `endbfchar` block.
///
/// Each entry has the form `<src_code> <dst_utf16>`.
fn parse_bfchar_section(parser: &mut CMapParser<'_>, map: &mut HashMap<u16, Vec<char>>) -> bool {
    enum State {
        NeedSource,
        NeedDestination(u16),
    }

    let mut state = State::NeedSource;

    loop {
        let token = match next_cmap_token(parser) {
            Ok(Some(token)) => token,
            Ok(None) => return true,
            Err(_) => return false,
        };

        match token {
            CMapToken::Operator(operator) if operator.as_slice() == b"endbfchar" => return true,
            CMapToken::HexString(bytes) => match state {
                State::NeedSource => {
                    state = State::NeedDestination(bytes_to_char_code(&bytes));
                }
                State::NeedDestination(source) => {
                    let chars = utf16_bytes_to_chars(&bytes);
                    if !chars.is_empty() {
                        map.insert(source, chars);
                    }
                    state = State::NeedSource;
                }
            },
            _ => {
                if matches!(state, State::NeedDestination(_))
                    && !consume_until_operator(parser, b"endbfchar").unwrap_or(false)
                {
                    return false;
                }
                state = State::NeedSource;
            }
        }
    }
}

/// Parse a `beginbfrange` ... `endbfrange` block.
///
/// Each entry is one of:
/// - `<c1> <c2> <base>` - sequential: c1 -> base, c1+1 -> base+1, ...
/// - `<c1> <c2> [<u1> <u2> ...]` - individual mappings for c1, c1+1, ...
fn parse_bfrange_section(parser: &mut CMapParser<'_>, map: &mut HashMap<u16, Vec<char>>) -> bool {
    enum State {
        NeedStart,
        NeedEnd(u16),
        NeedValue { start: u16, end: u16 },
        Array { code: u16, end: u16 },
    }

    let mut state = State::NeedStart;

    loop {
        let token = match next_cmap_token(parser) {
            Ok(Some(token)) => token,
            Ok(None) => return true,
            Err(_) => return false,
        };

        match token {
            CMapToken::Operator(operator) if operator.as_slice() == b"endbfrange" => return true,
            CMapToken::HexString(bytes) => match state {
                State::NeedStart => {
                    state = State::NeedEnd(bytes_to_char_code(&bytes));
                }
                State::NeedEnd(start) => {
                    state = State::NeedValue {
                        start,
                        end: bytes_to_char_code(&bytes),
                    };
                }
                State::NeedValue { start, end } => {
                    insert_sequential_range(map, start, end, &bytes);
                    state = State::NeedStart;
                }
                State::Array { code, end } => {
                    if let Some(chars) = utf16_bytes_to_chars_non_empty(&bytes) {
                        map.insert(code, chars);
                    }
                    if code >= end {
                        state = State::NeedStart;
                    } else {
                        state = State::Array {
                            code: code.saturating_add(1),
                            end,
                        };
                    }
                }
            },
            CMapToken::LeftSquareBracket => match state {
                State::NeedValue { start, end } => {
                    state = State::Array { code: start, end };
                }
                State::Array { .. } => {}
                _ => {
                    if !consume_until_operator(parser, b"endbfrange").unwrap_or(false) {
                        return false;
                    }
                    return true;
                }
            },
            CMapToken::RightSquareBracket => {
                if let State::Array { .. } = state {
                    state = State::NeedStart;
                }
            }
            _ => match state {
                State::NeedStart | State::Array { .. } => {}
                _ => {
                    if !consume_until_operator(parser, b"endbfrange").unwrap_or(false) {
                        return false;
                    }
                    return true;
                }
            },
        }
    }
}

fn insert_sequential_range(
    map: &mut HashMap<u16, Vec<char>>,
    start: u16,
    end: u16,
    base_bytes: &[u8],
) {
    let mut code = start;
    let mut base = bytes_to_u32(base_bytes);

    loop {
        let chars = codepoint_to_chars(base);
        if !chars.is_empty() {
            map.insert(code, chars);
        }
        if code >= end {
            break;
        }
        code = code.saturating_add(1);
        base = base.saturating_add(1);
    }
}

fn utf16_bytes_to_chars_non_empty(bytes: &[u8]) -> Option<Vec<char>> {
    let chars = utf16_bytes_to_chars(bytes);
    if chars.is_empty() { None } else { Some(chars) }
}

/// Convert 1-2 decoded bytes to a PDF character code (big-endian `u16`).
fn bytes_to_char_code(bytes: &[u8]) -> u16 {
    match bytes {
        [] => 0,
        [b] => u16::from(*b),
        [hi, lo] => u16::from(*hi) << 8 | u16::from(*lo),
        _ => {
            let n = bytes.len();
            let hi = bytes.get(n.saturating_sub(2)).copied().unwrap_or(0);
            let lo = bytes.get(n.saturating_sub(1)).copied().unwrap_or(0);
            u16::from(hi) << 8 | u16::from(lo)
        }
    }
}

/// Decode a UTF-16BE byte slice to a `Vec<char>`.
///
/// Handles surrogate pairs. Invalid code units are skipped.
fn utf16_bytes_to_chars(bytes: &[u8]) -> Vec<char> {
    let mut chars = Vec::new();
    let mut i = 0usize;
    while i.saturating_add(1) < bytes.len() {
        let hi = u16::from(*bytes.get(i).unwrap_or(&0));
        let lo = u16::from(*bytes.get(i.saturating_add(1)).unwrap_or(&0));
        let unit = (hi << 8) | lo;
        i = i.saturating_add(2);

        if (0xD800..=0xDBFF).contains(&unit) {
            if i.saturating_add(1) < bytes.len() {
                let h2 = u16::from(*bytes.get(i).unwrap_or(&0));
                let l2 = u16::from(*bytes.get(i.saturating_add(1)).unwrap_or(&0));
                let low = (h2 << 8) | l2;
                i = i.saturating_add(2);
                if (0xDC00..=0xDFFF).contains(&low) {
                    let high_bits = u32::from(unit & 0x3FF).wrapping_shl(10);
                    let low_bits = u32::from(low & 0x3FF);
                    let cp = 0x10000u32.wrapping_add(high_bits).wrapping_add(low_bits);
                    if let Some(c) = char::from_u32(cp) {
                        chars.push(c);
                    }
                }
            }
        } else if !matches!(unit, 0xDC00..=0xDFFF)
            && let Some(c) = char::from_u32(u32::from(unit))
        {
            chars.push(c);
        }
    }
    chars
}

/// Convert a raw Unicode codepoint (as `u32`) to a `Vec<char>`.
fn codepoint_to_chars(cp: u32) -> Vec<char> {
    char::from_u32(cp).map(|c| vec![c]).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bfchar_basic() {
        let cmap = br"
/CIDInit /ProcSet findresource begin
beginbfchar
<41> <0041>
<61> <0061>
endbfchar
end
";
        let map = ToUnicodeCMap::from_bytes(cmap);
        assert_eq!(map.map_char_code(0x41), Some(['\u{0041}'].as_slice()));
        assert_eq!(map.map_char_code(0x61), Some(['\u{0061}'].as_slice()));
        assert_eq!(map.map_char_code(0x42), None);
    }

    #[test]
    fn test_bfrange_sequential() {
        let cmap = br"
beginbfrange
<20> <39> <0020>
endbfrange
";
        let map = ToUnicodeCMap::from_bytes(cmap);
        assert_eq!(map.map_char_code(0x20), Some([' '].as_slice()));
        assert_eq!(map.map_char_code(0x39), Some(['9'].as_slice()));
        assert_eq!(map.map_char_code(0x3A), None);
    }

    #[test]
    fn test_bfrange_array() {
        let cmap = br"
beginbfrange
<0000> <0002> [<0041> <0042> <0043>]
endbfrange
";
        let map = ToUnicodeCMap::from_bytes(cmap);
        assert_eq!(map.map_char_code(0), Some(['A'].as_slice()));
        assert_eq!(map.map_char_code(1), Some(['B'].as_slice()));
        assert_eq!(map.map_char_code(2), Some(['C'].as_slice()));
    }

    #[test]
    fn test_surrogate_pair() {
        let cmap = b"beginbfchar\n<01> <D83DDE00>\nendbfchar\n";
        let map = ToUnicodeCMap::from_bytes(cmap);
        let chars = map.map_char_code(1);
        assert!(chars.is_some());
        assert_eq!(chars.unwrap().first().copied(), char::from_u32(0x1F600));
    }

    #[test]
    fn test_comments_odd_hex_and_malformed_entries_are_best_effort() {
        let cmap = br#"
beginbfchar
<01> % comment between tokens
<0041>
bad-token
<02> <041>
endbfchar

beginbfrange
<10> <12> [ % comment inside the array block
<0042> <0043> <0044>
]
malformed
<20> <22> <0045>
endbfrange
"#;
        let map = ToUnicodeCMap::from_bytes(cmap);

        assert_eq!(map.map_char_code(0x01), Some(['A'].as_slice()));
        assert_eq!(map.map_char_code(0x02), Some(['\u{0410}'].as_slice()));
        assert_eq!(map.map_char_code(0x10), Some(['B'].as_slice()));
        assert_eq!(map.map_char_code(0x11), Some(['C'].as_slice()));
        assert_eq!(map.map_char_code(0x12), Some(['D'].as_slice()));
        assert_eq!(map.map_char_code(0x20), Some(['E'].as_slice()));
        assert_eq!(map.map_char_code(0x21), Some(['F'].as_slice()));
        assert_eq!(map.map_char_code(0x22), Some(['G'].as_slice()));
    }
}
