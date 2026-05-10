use std::collections::{BTreeSet, HashMap};

use crate::error::FontError;

/// Writing mode declared by a Type0 encoding CMap.
///
/// This controls whether glyph metrics should be interpreted horizontally or
/// vertically. The current parser preserves the mode for callers, while text
/// rendering continues to target horizontal layout unless higher layers add
/// separate vertical-metrics handling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WritingMode {
    /// Horizontal writing mode (`/WMode 0`).
    Horizontal,
    /// Vertical writing mode (`/WMode 1`).
    Vertical,
}

/// Parsed representation of a Type0 font `/Encoding` CMap.
///
/// A Type0 font may reference either a predefined identity CMap by name or an
/// embedded CMap stream that maps variable-length byte sequences to CIDs.
#[derive(Debug, Clone)]
pub enum Type0EncodingCMap {
    /// Predefined identity mapping with an associated writing mode.
    Identity { writing_mode: WritingMode },
    /// Embedded CMap data parsed from a stream object.
    Embedded(EmbeddedCMap),
}

/// Parsed data for an embedded Type0 encoding CMap stream.
#[derive(Debug, Clone)]
pub struct EmbeddedCMap {
    code_space_ranges: Vec<CodeSpaceRange>,
    allowed_code_lengths: Vec<usize>,
    cid_chars: HashMap<u32, u16>,
    cid_ranges: Vec<CidRange>,
    writing_mode: WritingMode,
}

/// Single codespace range describing which byte lengths and values are valid.
#[derive(Debug, Clone, Copy)]
struct CodeSpaceRange {
    start: u32,
    end: u32,
    len: usize,
}

/// Inclusive code range that maps sequential character codes onto sequential CIDs.
#[derive(Debug, Clone, Copy)]
struct CidRange {
    start: u32,
    end: u32,
    cid_start: u16,
}

impl Type0EncodingCMap {
    /// Build a Type0 encoding CMap from a predefined CMap name.
    ///
    /// Currently only the predefined identity CMaps are supported.
    pub fn from_name(name: &str) -> Result<Self, FontError> {
        match name {
            "Identity-H" => Ok(Self::Identity {
                writing_mode: WritingMode::Horizontal,
            }),
            "Identity-V" => Ok(Self::Identity {
                writing_mode: WritingMode::Vertical,
            }),
            _ => Err(FontError::UnsupportedType0EncodingCMap(name.to_string())),
        }
    }

    /// Parse an embedded Type0 encoding CMap stream.
    ///
    /// This currently supports:
    /// - `begincodespacerange`
    /// - `begincidchar`
    /// - `begincidrange`
    /// - `/WMode`
    pub fn from_bytes(data: &[u8]) -> Result<Self, FontError> {
        let text = strip_comments(&String::from_utf8_lossy(data));
        let tokens: Vec<&str> = text.split_whitespace().collect();

        let mut code_space_ranges = Vec::new();
        let mut code_lengths = BTreeSet::new();
        let mut cid_chars = HashMap::new();
        let mut cid_ranges = Vec::new();
        let mut writing_mode = WritingMode::Horizontal;

        let mut index = 0usize;
        while let Some(token) = tokens.get(index) {
            match *token {
                "begincodespacerange" => {
                    index = index.saturating_add(1);
                    while let Some(token) = tokens.get(index) {
                        if *token == "endcodespacerange" {
                            break;
                        }

                        let start = parse_hex_token(token).ok_or_else(|| {
                            FontError::InvalidType0EncodingCMap(
                                "invalid codespace range start token".to_string(),
                            )
                        })?;
                        let end = tokens
                            .get(index.saturating_add(1))
                            .and_then(|value| parse_hex_token(value))
                            .ok_or_else(|| {
                                FontError::InvalidType0EncodingCMap(
                                    "invalid codespace range end token".to_string(),
                                )
                            })?;

                        let len = hex_token_len(token).ok_or_else(|| {
                            FontError::InvalidType0EncodingCMap(
                                "invalid codespace range length".to_string(),
                            )
                        })?;

                        code_lengths.insert(len);
                        code_space_ranges.push(CodeSpaceRange { start, end, len });
                        index = index.saturating_add(2);
                    }
                }
                "begincidchar" => {
                    index = index.saturating_add(1);
                    while let Some(token) = tokens.get(index) {
                        if *token == "endcidchar" {
                            break;
                        }

                        let code = parse_hex_token(token).ok_or_else(|| {
                            FontError::InvalidType0EncodingCMap(
                                "invalid cidchar source token".to_string(),
                            )
                        })?;
                        let cid = tokens
                            .get(index.saturating_add(1))
                            .and_then(|value| value.parse::<u16>().ok())
                            .ok_or_else(|| {
                                FontError::InvalidType0EncodingCMap(
                                    "invalid cidchar destination token".to_string(),
                                )
                            })?;
                        cid_chars.insert(code, cid);
                        index = index.saturating_add(2);
                    }
                }
                "begincidrange" => {
                    index = index.saturating_add(1);
                    while let Some(token) = tokens.get(index) {
                        if *token == "endcidrange" {
                            break;
                        }

                        let start = parse_hex_token(token).ok_or_else(|| {
                            FontError::InvalidType0EncodingCMap(
                                "invalid cidrange start token".to_string(),
                            )
                        })?;
                        let end = tokens
                            .get(index.saturating_add(1))
                            .and_then(|value| parse_hex_token(value))
                            .ok_or_else(|| {
                                FontError::InvalidType0EncodingCMap(
                                    "invalid cidrange end token".to_string(),
                                )
                            })?;
                        let cid_start = tokens
                            .get(index.saturating_add(2))
                            .and_then(|value| value.parse::<u16>().ok())
                            .ok_or_else(|| {
                                FontError::InvalidType0EncodingCMap(
                                    "invalid cidrange destination token".to_string(),
                                )
                            })?;
                        cid_ranges.push(CidRange {
                            start,
                            end,
                            cid_start,
                        });
                        index = index.saturating_add(3);
                    }
                }
                "/WMode" => {
                    let mode = tokens
                        .get(index.saturating_add(1))
                        .and_then(|value| value.parse::<u8>().ok())
                        .unwrap_or(0);
                    writing_mode = if mode == 1 {
                        WritingMode::Vertical
                    } else {
                        WritingMode::Horizontal
                    };
                    index = index.saturating_add(1);
                }
                _ => {}
            }

            index = index.saturating_add(1);
        }

        if code_space_ranges.is_empty() {
            return Err(FontError::InvalidType0EncodingCMap(
                "missing codespace ranges".to_string(),
            ));
        }

        Ok(Self::Embedded(EmbeddedCMap {
            code_space_ranges,
            allowed_code_lengths: code_lengths.into_iter().collect(),
            cid_chars,
            cid_ranges,
            writing_mode,
        }))
    }

    /// Decode raw text bytes into CIDs using this CMap.
    pub fn decode(&self, text: &[u8]) -> Vec<u16> {
        match self {
            Self::Identity { .. } => decode_identity(text),
            Self::Embedded(cmap) => cmap.decode(text),
        }
    }

    /// Return whether this CMap is one of the predefined identity mappings.
    pub fn is_identity(&self) -> bool {
        matches!(self, Self::Identity { .. })
    }

    /// Return the writing mode declared by this CMap.
    pub fn writing_mode(&self) -> WritingMode {
        match self {
            Self::Identity { writing_mode } => *writing_mode,
            Self::Embedded(cmap) => cmap.writing_mode,
        }
    }
}

impl EmbeddedCMap {
    /// Decode raw text bytes into CIDs using this embedded CMap.
    ///
    /// The decoder prefers the longest valid codespace match at each position.
    /// If no valid codespace range matches, CID 0 is emitted and one byte is
    /// consumed so the parser can continue.
    fn decode(&self, text: &[u8]) -> Vec<u16> {
        let mut decoded = Vec::new();
        let mut position = 0usize;

        while position < text.len() {
            let mut matched = None;

            for len in self.allowed_code_lengths.iter().rev() {
                let end = position.saturating_add(*len);
                let Some(bytes) = text.get(position..end) else {
                    continue;
                };

                let code = bytes_to_u32(bytes);
                if self
                    .code_space_ranges
                    .iter()
                    .any(|range| range.len == *len && range.start <= code && code <= range.end)
                {
                    matched = Some((*len, self.map_code_to_cid(code).unwrap_or(0)));
                    break;
                }
            }

            if let Some((len, cid)) = matched {
                decoded.push(cid);
                position = position.saturating_add(len);
            } else {
                decoded.push(0);
                position = position.saturating_add(1);
            }
        }

        decoded
    }

    /// Map one parsed character code to its CID.
    fn map_code_to_cid(&self, code: u32) -> Option<u16> {
        if let Some(cid) = self.cid_chars.get(&code) {
            return Some(*cid);
        }

        self.cid_ranges.iter().find_map(|range| {
            if range.start <= code && code <= range.end {
                let offset = code.saturating_sub(range.start);
                let offset = u16::try_from(offset).ok()?;
                range.cid_start.checked_add(offset)
            } else {
                None
            }
        })
    }
}

/// Decode bytes using the predefined identity composite-font convention.
///
/// Identity CMaps interpret each two-byte big-endian unit as a CID. An odd
/// trailing byte yields CID 0.
fn decode_identity(text: &[u8]) -> Vec<u16> {
    let mut decoded = Vec::new();
    let mut chunks = text.chunks_exact(2);

    for pair in &mut chunks {
        let Some(first) = pair.first().copied() else {
            continue;
        };
        let Some(second) = pair.get(1).copied() else {
            continue;
        };
        decoded.push(u16::from_be_bytes([first, second]));
    }

    if !chunks.remainder().is_empty() {
        decoded.push(0);
    }

    decoded
}

/// Remove `%` line comments from a PostScript-like CMap source string.
fn strip_comments(text: &str) -> String {
    let mut stripped = String::new();

    for line in text.lines() {
        let line = line.split('%').next().unwrap_or_default();
        stripped.push_str(line);
        stripped.push('\n');
    }

    stripped
}

/// Parse a `<...>` hexadecimal token into an unsigned integer code value.
fn parse_hex_token(token: &str) -> Option<u32> {
    let inner = token.strip_prefix('<')?.strip_suffix('>')?;
    let mut value = 0u32;

    for pair in hex_bytes(inner)? {
        value = value.checked_shl(8)? | u32::from(pair);
    }

    Some(value)
}

/// Return the decoded byte length of a `<...>` hexadecimal token.
fn hex_token_len(token: &str) -> Option<usize> {
    Some(hex_bytes(token.strip_prefix('<')?.strip_suffix('>')?)?.len())
}

/// Decode a hexadecimal string into bytes.
///
/// Non-hex characters are ignored and odd-length inputs are left-padded with
/// zero to follow PDF hex-string rules.
fn hex_bytes(hex: &str) -> Option<Vec<u8>> {
    let hex: String = hex.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    let padded = if hex.len() % 2 == 0 {
        hex
    } else {
        let mut prefixed = String::with_capacity(hex.len().saturating_add(1));
        prefixed.push('0');
        prefixed.push_str(&hex);
        prefixed
    };

    let mut bytes = Vec::with_capacity(padded.len() / 2);
    let chars: Vec<char> = padded.chars().collect();
    let mut index = 0usize;
    while let (Some(high), Some(low)) = (chars.get(index), chars.get(index.saturating_add(1))) {
        let high = high.to_digit(16)?;
        let low = low.to_digit(16)?;
        let byte = u8::try_from((high << 4) | low).ok()?;
        bytes.push(byte);
        index = index.saturating_add(2);
    }

    Some(bytes)
}

/// Convert a big-endian byte sequence into a `u32`.
fn bytes_to_u32(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0u32, |value, byte| {
        value.checked_shl(8).unwrap_or(0) | u32::from(*byte)
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn identity_h_decodes_big_endian_pairs() {
        let cmap = Type0EncodingCMap::from_name("Identity-H").unwrap();

        assert_eq!(cmap.decode(&[0x00, 0x01, 0x12, 0x34]), vec![1, 0x1234]);
        assert!(cmap.is_identity());
        assert_eq!(cmap.writing_mode(), WritingMode::Horizontal);
    }

    #[test]
    fn embedded_cmap_decodes_cidchar_and_cidrange_entries() {
        let data = br#"
        begincmap
        /WMode 0 def
        2 begincodespacerange
        <00> <FF>
        <0100> <01FF>
        endcodespacerange
        1 begincidchar
        <20> 7
        endcidchar
        1 begincidrange
        <0100> <0102> 50
        endcidrange
        endcmap
        "#;

        let cmap = Type0EncodingCMap::from_bytes(data).unwrap();

        assert_eq!(
            cmap.decode(&[0x20, 0x01, 0x00, 0x01, 0x02]),
            vec![7, 50, 52]
        );
        assert_eq!(cmap.writing_mode(), WritingMode::Horizontal);
    }

    #[test]
    fn embedded_cmap_uses_notdef_for_unmapped_or_invalid_codes() {
        let data = br#"
        begincmap
        /WMode 1 def
        1 begincodespacerange
        <0001> <00FF>
        endcodespacerange
        1 begincidchar
        <0001> 4
        endcidchar
        endcmap
        "#;

        let cmap = Type0EncodingCMap::from_bytes(data).unwrap();

        assert_eq!(cmap.decode(&[0x00, 0x01, 0x00, 0x02, 0xFF]), vec![4, 0, 0]);
        assert_eq!(cmap.writing_mode(), WritingMode::Vertical);
    }
}
