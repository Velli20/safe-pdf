use pdf_object::{stream::StreamObject, traits::FromStreamObject};
use std::collections::HashMap;
use thiserror::Error;

/// Errors that can occur during CMap parsing.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum CMapError {
    /// The hexadecimal string has an odd number of characters, making it invalid.
    #[error("Hex string '{0}' has an odd number of characters")]
    OddHexLength(String),
    /// The string contains an invalid hexadecimal sequence.
    #[error("Invalid hex sequence '{original_hex}' in CMap: {source}")]
    InvalidHexSequence {
        original_hex: String,
        #[source]
        source: std::num::ParseIntError,
    },
    /// The parsed u32 value from a hex string is not a valid Unicode scalar value.
    #[error(
        "Hex value <{hex_value}> ({u32_value}) is not a valid Unicode scalar value for a CMap character"
    )]
    InvalidUnicodeScalar { hex_value: String, u32_value: u32 },
}

/// Parses a hexadecimal string representation into a u32 value.
/// Example: "010A" -> 0x010A (266 decimal)
fn parse_hex_to_u32(hex: &str) -> Result<u32, CMapError> {
    u32::from_str_radix(hex, 16).map_err(|e| CMapError::InvalidHexSequence {
        original_hex: hex.to_string(),
        source: e,
    })
}

#[derive(Debug, Default)]
pub struct CharacterMap {
    /// Mappings from source character codes to destination character codes (often CIDs or Unicode).
    /// These are primarily from `bfchar` entries.
    pub bfchar_mappings: HashMap<u32, char>,
}

impl CharacterMap {
    pub fn new() -> Self {
        Self::default()
    }

    /// Retrieves a mapping for a given character code.
    pub fn get_mapping(&self, char_code: u32) -> Option<char> {
        self.bfchar_mappings.get(&char_code).cloned()
    }
}

impl FromStreamObject for CharacterMap {
    type ResultType = Self;
    type ErrorType = CMapError;

    fn from_stream_object(stream: &StreamObject) -> Result<Self::ResultType, Self::ErrorType> {
        let content = String::from_utf8_lossy(&stream.data);
        let mut bfchar_mappings = HashMap::new();
        let mut in_bfchar_block = false;

        for line in content.lines() {
            let trimmed_line = line.trim();

            // Skip comments and empty lines.
            if trimmed_line.is_empty() || trimmed_line.starts_with('%') {
                continue;
            }

            if trimmed_line.contains("beginbfchar") {
                in_bfchar_block = true;
                continue;
            }

            if trimmed_line.contains("endbfchar") {
                in_bfchar_block = false;
                continue;
            }

            if in_bfchar_block {
                let parts: Vec<&str> = trimmed_line.split_whitespace().collect();
                if parts.len() == 2
                    && parts[0].starts_with('<')
                    && parts[0].ends_with('>')
                    && parts[0].len() > 2
                    && parts[1].starts_with('<')
                    && parts[1].ends_with('>')
                    && parts[1].len() > 2
                {
                    let src_hex = &parts[0][1..parts[0].len() - 1];
                    let dst_hex = &parts[1][1..parts[1].len() - 1];

                    let src_code = parse_hex_to_u32(src_hex)?;
                    let dst_u32 = parse_hex_to_u32(dst_hex)?;

                    let dst_char =
                        char::from_u32(dst_u32).ok_or_else(|| CMapError::InvalidUnicodeScalar {
                            hex_value: dst_hex.to_string(),
                            u32_value: dst_u32,
                        })?;

                    bfchar_mappings.insert(src_code, dst_char);
                }
            }
        }
        Ok(Self { bfchar_mappings })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_object::dictionary::Dictionary;
    use std::{collections::BTreeMap, rc::Rc};

    #[test]
    fn test_character_map_parsing() {
        let input_data = b"/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n/CIDSystemInfo <<\n   /Registry (Adobe)\n   /Ordering (UCS)\n   /Supplement 0\n>> def\n/CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n1 begincodespacerange\n<00><7F>\nendcodespacerange\n9 beginbfchar\n<01> <0020>\n<02> <0048>\n<03> <0065>\n<04> <006C>\n<05> <006F>\n<06> <0057>\n<07> <0072>\n<08> <0064>\n<09> <0021>\nendbfchar\nendcmap\nCMapName currentdict /CMap defineresource pop\nend\nend".to_vec();

        let dictionary = Dictionary::new(BTreeMap::new());
        let stream = StreamObject::new(0, 0, Rc::new(dictionary), input_data);

        let cmap = CharacterMap::from_stream_object(&stream).expect("CMap parsing failed");

        assert_eq!(
            cmap.bfchar_mappings.len(),
            9,
            "Should parse 9 bfchar entries"
        );

        assert_eq!(cmap.get_mapping(0x01_u32), Some(' ')); // <01> -> <0020> (Space)
        assert_eq!(cmap.get_mapping(0x02_u32), Some('H')); // <02> -> <0048> (H)
        assert_eq!(cmap.get_mapping(0x03_u32), Some('e')); // <03> -> <0065> (e)
        assert_eq!(cmap.get_mapping(0x04_u32), Some('l')); // <04> -> <006C> (l)
        assert_eq!(cmap.get_mapping(0x05_u32), Some('o')); // <05> -> <006F> (o)
        assert_eq!(cmap.get_mapping(0x06_u32), Some('W')); // <06> -> <005W> (W)
        assert_eq!(cmap.get_mapping(0x07_u32), Some('r')); // <07> -> <0072> (r)
        assert_eq!(cmap.get_mapping(0x08_u32), Some('d')); // <08> -> <0064> (d)
        assert_eq!(cmap.get_mapping(0x09_u32), Some('!')); // <09> -> <0021> (!)

        // Test a non-existent mapping
        assert_eq!(cmap.get_mapping(0x0A_u32), None);
    }
}
