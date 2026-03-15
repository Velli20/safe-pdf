use std::collections::HashMap;

/// A parsed ToUnicode CMap that maps PDF character codes to Unicode scalar values.
///
/// A ToUnicode CMap is embedded as a stream in a font dictionary.  It uses a
/// PostScript-like syntax with `beginbfchar`/`endbfchar` and
/// `beginbfrange`/`endbfrange` sections to declare the mapping.
#[derive(Debug)]
pub struct ToUnicodeCMap(HashMap<u16, Vec<char>>);

impl ToUnicodeCMap {
    /// Parse a ToUnicode CMap stream.
    ///
    /// Only the `bfchar` and `bfrange` sections are processed; the rest of the
    /// PostScript-like prologue is ignored.  Invalid or unrecognised entries are
    /// silently skipped so that partial CMaps still yield useful results.
    pub fn from_bytes(data: &[u8]) -> Self {
        let mut map: HashMap<u16, Vec<char>> = HashMap::new();

        // Work on the raw bytes as ASCII-compatible text.  Non-UTF-8 data is
        // tolerated by falling back to lossy conversion; the hex tokens we care
        // about are always ASCII.
        let text = String::from_utf8_lossy(data);
        let text: &str = &text;

        // ---- bfchar sections ----
        let mut remainder = text;
        loop {
            let Some(start) = remainder.find("beginbfchar") else {
                break;
            };
            remainder = &remainder[start.saturating_add("beginbfchar".len())..];
            let end = remainder.find("endbfchar").unwrap_or(remainder.len());
            let section = &remainder[..end];
            parse_bfchar(section, &mut map);
            // Advance past the closing tag if present
            if end < remainder.len() {
                remainder = &remainder[end.saturating_add("endbfchar".len())..];
            } else {
                break;
            }
        }

        // ---- bfrange sections ----
        let mut remainder = text;
        loop {
            let Some(start) = remainder.find("beginbfrange") else {
                break;
            };
            remainder = &remainder[start.saturating_add("beginbfrange".len())..];
            let end = remainder.find("endbfrange").unwrap_or(remainder.len());
            let section = &remainder[..end];
            parse_bfrange(section, &mut map);
            if end < remainder.len() {
                remainder = &remainder[end.saturating_add("endbfrange".len())..];
            } else {
                break;
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

/// Parse a `beginbfchar` … `endbfchar` block.
///
/// Each entry has the form `<src_code> <dst_utf16>`.
fn parse_bfchar(section: &str, map: &mut HashMap<u16, Vec<char>>) {
    let mut tokens = hex_tokens(section);
    loop {
        let Some(src_bytes) = tokens.next() else {
            break;
        };
        let Some(dst_bytes) = tokens.next() else {
            break;
        };
        let src = bytes_to_char_code(&src_bytes);
        let chars = utf16_bytes_to_chars(&dst_bytes);
        if !chars.is_empty() {
            map.insert(src, chars);
        }
    }
}

/// Parse a `beginbfrange` … `endbfrange` block.
///
/// Each entry is one of:
/// - `<c1> <c2> <base>` – sequential: c1 → base, c1+1 → base+1, …
/// - `<c1> <c2> [<u1> <u2> …]` – individual mappings for c1, c1+1, …
fn parse_bfrange(section: &str, map: &mut HashMap<u16, Vec<char>>) {
    let mut pos = 0usize;
    let bytes = section.as_bytes();

    loop {
        // Skip whitespace and comments
        pos = skip_whitespace(bytes, pos);
        if pos >= bytes.len() {
            break;
        }

        // Read c1 hex token
        let (c1_bytes, next) = match read_hex_token(bytes, pos) {
            Some(v) => v,
            None => break,
        };
        pos = skip_whitespace(bytes, next);

        // Read c2 hex token
        let (c2_bytes, next) = match read_hex_token(bytes, pos) {
            Some(v) => v,
            None => break,
        };
        pos = skip_whitespace(bytes, next);

        let c1 = bytes_to_char_code(&c1_bytes);
        let c2 = bytes_to_char_code(&c2_bytes);

        // Check for array form `[...]`
        if bytes.get(pos) == Some(&b'[') {
            // Array form: collect each hex token until `]`
            pos = pos.saturating_add(1); // skip `[`
            let mut code = c1;
            loop {
                pos = skip_whitespace(bytes, pos);
                if bytes.get(pos) == Some(&b']') || pos >= bytes.len() {
                    pos = pos.saturating_add(1); // skip `]`
                    break;
                }
                let Some((u_bytes, next)) = read_hex_token(bytes, pos) else {
                    break;
                };
                pos = next;
                let chars = utf16_bytes_to_chars(&u_bytes);
                if !chars.is_empty() {
                    map.insert(code, chars);
                }
                if code >= c2 {
                    break;
                }
                code = code.saturating_add(1);
            }
        } else {
            // Sequential form: `<base>` hex token
            let Some((base_bytes, next)) = read_hex_token(bytes, pos) else {
                break;
            };
            pos = next;
            let mut base = bytes_to_u32(&base_bytes);
            let mut code = c1;
            loop {
                // Decode current codepoint from base as UTF-16
                let chars = codepoint_to_chars(base);
                if !chars.is_empty() {
                    map.insert(code, chars);
                }
                if code >= c2 {
                    break;
                }
                code = code.saturating_add(1);
                base = base.saturating_add(1);
            }
        }
    }
}

/// An iterator that yields hex token byte-vectors from `<XX...>` sequences.
struct HexTokens<'a> {
    src: &'a str,
    pos: usize,
}

fn hex_tokens(src: &str) -> HexTokens<'_> {
    HexTokens { src, pos: 0 }
}

impl Iterator for HexTokens<'_> {
    type Item = Vec<u8>;

    fn next(&mut self) -> Option<Vec<u8>> {
        // Scan for valid `<XXXX>` hex tokens, skipping any that fail to decode.
        'search: loop {
            let rest = self.src.get(self.pos..)?;
            let rel_open = rest.find('<')?;
            let open = self.pos.saturating_add(rel_open);
            let after_open = open.saturating_add(1);
            let rest2 = self.src.get(after_open..)?;
            let rel_close = rest2.find('>')?;
            let close = after_open.saturating_add(rel_close);
            self.pos = close.saturating_add(1);
            let Some(hex_str) = self.src.get(after_open..close) else {
                continue 'search;
            };
            if let Some(decoded) = decode_hex_str(hex_str) {
                return Some(decoded);
            }
            // decode_hex_str failed — skip this token and try the next one
        }
    }
}

/// Skip ASCII whitespace and `%`-style comments in `buf` starting at `pos`.
fn skip_whitespace(buf: &[u8], mut pos: usize) -> usize {
    while pos < buf.len() {
        match buf.get(pos) {
            Some(&b' ') | Some(&b'\t') | Some(&b'\n') | Some(&b'\r') => {
                pos = pos.saturating_add(1);
            }
            Some(&b'%') => {
                // Skip to end of line
                while pos < buf.len() && buf.get(pos) != Some(&b'\n') {
                    pos = pos.saturating_add(1);
                }
            }
            _ => break,
        }
    }
    pos
}

/// Read a `<XXXX>` hex token from `buf` at `pos`.
///
/// Returns `(decoded_bytes, position_after_closing_angle)` or `None`.
fn read_hex_token(buf: &[u8], pos: usize) -> Option<(Vec<u8>, usize)> {
    let open = buf.get(pos)?;
    if *open != b'<' {
        return None;
    }
    let start = pos.saturating_add(1);
    let close = buf
        .get(start..)?
        .iter()
        .position(|&b| b == b'>')?
        .saturating_add(start);
    let hex_bytes = buf.get(start..close)?;
    let hex_str = std::str::from_utf8(hex_bytes).ok()?;
    let decoded = decode_hex_str(hex_str)?;
    Some((decoded, close.saturating_add(1)))
}

/// Decode a hex string (without angle brackets) into bytes.
///
/// Handles both even and odd-length hex strings (pads with a leading zero if
/// the length is odd, per the PDF spec).
fn decode_hex_str(s: &str) -> Option<Vec<u8>> {
    let s = s.trim();
    // Ignore non-hex characters (e.g. whitespace inside <> tokens)
    let hex: String = s.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    // Pad to even length
    let padded = if hex.len() % 2 == 0 {
        hex
    } else {
        let mut p = String::with_capacity(hex.len().saturating_add(1));
        p.push('0');
        p.push_str(&hex);
        p
    };
    let mut out = Vec::with_capacity(padded.len() / 2);
    let mut iter = padded.chars();
    loop {
        let hi = iter.next()?;
        let lo = iter.next()?;
        let byte = (hex_digit(hi)? << 4) | hex_digit(lo)?;
        out.push(byte);
        if iter.as_str().is_empty() {
            break;
        }
    }
    Some(out)
}

fn hex_digit(c: char) -> Option<u8> {
    match c {
        '0'..='9' => u32::from(c)
            .checked_sub(u32::from('0'))
            .and_then(|v| u8::try_from(v).ok()),
        'a'..='f' => u32::from(c)
            .checked_sub(u32::from('a'))
            .and_then(|v| u8::try_from(v.saturating_add(10)).ok()),
        'A'..='F' => u32::from(c)
            .checked_sub(u32::from('A'))
            .and_then(|v| u8::try_from(v.saturating_add(10)).ok()),
        _ => None,
    }
}

/// Convert 1–2 decoded bytes to a PDF character code (big-endian `u16`).
fn bytes_to_char_code(bytes: &[u8]) -> u16 {
    match bytes {
        [] => 0,
        [b] => u16::from(*b),
        [hi, lo] => u16::from(*hi) << 8 | u16::from(*lo),
        _ => {
            // More than 2 bytes: take the last two
            let n = bytes.len();
            let hi = *bytes.get(n.wrapping_sub(2)).unwrap_or(&0);
            let lo = *bytes.get(n.wrapping_sub(1)).unwrap_or(&0);
            u16::from(hi) << 8 | u16::from(lo)
        }
    }
}

/// Interpret a decoded byte slice as a big-endian `u32` codepoint or surrogate pair.
fn bytes_to_u32(bytes: &[u8]) -> u32 {
    let b = |i: usize| u32::from(*bytes.get(i).unwrap_or(&0));
    match bytes.len() {
        0 => 0,
        1 => b(0),
        2 => (b(0) << 8) | b(1),
        3 => (b(0) << 16) | (b(1) << 8) | b(2),
        _ => (b(0) << 24) | (b(1) << 16) | (b(2) << 8) | b(3),
    }
}

/// Decode a UTF-16BE byte slice to a `Vec<char>`.
///
/// Handles surrogate pairs.  Invalid code units are skipped.
fn utf16_bytes_to_chars(bytes: &[u8]) -> Vec<char> {
    let mut chars = Vec::new();
    let mut i = 0usize;
    while i.saturating_add(1) < bytes.len() {
        let hi = u16::from(*bytes.get(i).unwrap_or(&0));
        let lo = u16::from(*bytes.get(i.saturating_add(1)).unwrap_or(&0));
        let unit = (hi << 8) | lo;
        i = i.saturating_add(2);

        if (0xD800..=0xDBFF).contains(&unit) {
            // High surrogate — expect low surrogate next
            if i.saturating_add(1) < bytes.len() {
                let h2 = u16::from(*bytes.get(i).unwrap_or(&0));
                let l2 = u16::from(*bytes.get(i.saturating_add(1)).unwrap_or(&0));
                let low = (h2 << 8) | l2;
                i = i.saturating_add(2);
                if (0xDC00..=0xDFFF).contains(&low) {
                    // Surrogate pair decoding (no overflow possible: max result = 0x10FFFF)
                    let high_bits = u32::from(unit & 0x3FF).wrapping_shl(10);
                    let low_bits = u32::from(low & 0x3FF);
                    let cp = 0x10000u32.wrapping_add(high_bits).wrapping_add(low_bits);
                    if let Some(c) = char::from_u32(cp) {
                        chars.push(c);
                    }
                }
            }
        } else if (0xDC00..=0xDFFF).contains(&unit) {
            // Stray low surrogate — skip
        } else if let Some(c) = char::from_u32(u32::from(unit)) {
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
        // 0x20 → U+0020 (space), 0x39 → U+0039 ('9')
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
        // U+1F600 encoded as surrogate pair D83D DE00
        let cmap = b"beginbfchar\n<01> <D83DDE00>\nendbfchar\n";
        let map = ToUnicodeCMap::from_bytes(cmap);
        let chars = map.map_char_code(1);
        assert!(chars.is_some());
        // U+1F600 = 128512
        assert_eq!(chars.unwrap().first().copied(), char::from_u32(0x1F600));
    }
}
