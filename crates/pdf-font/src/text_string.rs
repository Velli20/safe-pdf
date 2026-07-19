use pdf_object::text_encoding::BigEndianU16Units;
use thiserror::Error;

const UTF16_BIG_ENDIAN_BOM: &[u8] = &[0xFE, 0xFF];
const UTF8_BOM: &[u8] = &[0xEF, 0xBB, 0xBF];

/// Errors produced while decoding a PDF text string.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum TextStringError {
    /// A UTF-16BE string ends with an incomplete code unit.
    #[error("UTF-16BE PDF text string has an incomplete trailing code unit")]
    IncompleteUtf16CodeUnit,
    /// A UTF-16BE string contains an invalid surrogate sequence.
    #[error("PDF text string contains invalid UTF-16BE")]
    InvalidUtf16,
    /// A BOM-marked UTF-8 string contains invalid UTF-8.
    #[error("PDF text string contains invalid UTF-8")]
    InvalidUtf8,
    /// A byte has no character assigned in PDFDocEncoding.
    #[error("byte 0x{byte:02X} is undefined in PDFDocEncoding")]
    UndefinedPdfDocEncodingByte { byte: u8 },
}

/// Decodes a PDF text string into Unicode.
///
/// UTF-16BE and UTF-8 strings are identified by their required byte-order
/// marks. An unmarked string is decoded using PDFDocEncoding.
pub fn decode(bytes: &[u8]) -> Result<String, TextStringError> {
    if let Some(body) = bytes.strip_prefix(UTF16_BIG_ENDIAN_BOM) {
        return decode_utf16_big_endian(body);
    }
    if let Some(body) = bytes.strip_prefix(UTF8_BOM) {
        return std::str::from_utf8(body)
            .map(str::to_owned)
            .map_err(|_| TextStringError::InvalidUtf8);
    }

    decode_pdf_doc_encoding(bytes)
}

/// Encodes Unicode as a byte-order-marked UTF-16BE PDF text string.
pub fn encode(text: &str) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(text.len().saturating_mul(2).saturating_add(2));
    bytes.extend_from_slice(UTF16_BIG_ENDIAN_BOM);
    for unit in text.encode_utf16() {
        bytes.extend_from_slice(&unit.to_be_bytes());
    }
    bytes
}

fn decode_utf16_big_endian(bytes: &[u8]) -> Result<String, TextStringError> {
    let decoded = BigEndianU16Units::from(bytes);
    if decoded.trailing_byte.is_some() {
        return Err(TextStringError::IncompleteUtf16CodeUnit);
    }

    String::from_utf16(&decoded.units).map_err(|_| TextStringError::InvalidUtf16)
}

fn decode_pdf_doc_encoding(bytes: &[u8]) -> Result<String, TextStringError> {
    bytes.iter().copied().map(decode_pdf_doc_byte).collect()
}

fn decode_pdf_doc_byte(byte: u8) -> Result<char, TextStringError> {
    let character = match byte {
        0x18 => '\u{02D8}',
        0x19 => '\u{02C7}',
        0x1A => '\u{02C6}',
        0x1B => '\u{02D9}',
        0x1C => '\u{02DD}',
        0x1D => '\u{02DB}',
        0x1E => '\u{02DA}',
        0x1F => '\u{02DC}',
        0x7F | 0x9F | 0xAD => {
            return Err(TextStringError::UndefinedPdfDocEncodingByte { byte });
        }
        0x80 => '\u{2022}',
        0x81 => '\u{2020}',
        0x82 => '\u{2021}',
        0x83 => '\u{2026}',
        0x84 => '\u{2014}',
        0x85 => '\u{2013}',
        0x86 => '\u{0192}',
        0x87 => '\u{2044}',
        0x88 => '\u{2039}',
        0x89 => '\u{203A}',
        0x8A => '\u{2212}',
        0x8B => '\u{2030}',
        0x8C => '\u{201E}',
        0x8D => '\u{201C}',
        0x8E => '\u{201D}',
        0x8F => '\u{2018}',
        0x90 => '\u{2019}',
        0x91 => '\u{201A}',
        0x92 => '\u{2122}',
        0x93 => '\u{FB01}',
        0x94 => '\u{FB02}',
        0x95 => '\u{0141}',
        0x96 => '\u{0152}',
        0x97 => '\u{0160}',
        0x98 => '\u{0178}',
        0x99 => '\u{017D}',
        0x9A => '\u{0131}',
        0x9B => '\u{0142}',
        0x9C => '\u{0153}',
        0x9D => '\u{0161}',
        0x9E => '\u{017E}',
        0xA0 => '\u{20AC}',
        _ => char::from_u32(u32::from(byte))
            .ok_or(TextStringError::UndefinedPdfDocEncodingByte { byte })?,
    };
    Ok(character)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16_big_endian_round_trips_bmp_and_supplementary_characters() {
        let text = "caf\u{e9} \u{1f642}";
        assert_eq!(decode(&encode(text)), Ok(text.to_owned()));
    }

    #[test]
    fn rejects_incomplete_and_invalid_utf16() {
        assert_eq!(
            decode(&[0xFE, 0xFF, 0x00]),
            Err(TextStringError::IncompleteUtf16CodeUnit)
        );
        assert_eq!(
            decode(&[0xFE, 0xFF, 0xD8, 0x3D]),
            Err(TextStringError::InvalidUtf16)
        );
    }

    #[test]
    fn decodes_bom_marked_utf8() {
        assert_eq!(
            decode(&[0xEF, 0xBB, 0xBF, 0xE2, 0x82, 0xAC]),
            Ok("\u{20AC}".to_owned())
        );
        assert_eq!(
            decode(&[0xEF, 0xBB, 0xBF, 0xFF]),
            Err(TextStringError::InvalidUtf8)
        );
    }

    #[test]
    fn decodes_pdf_doc_encoding() {
        assert_eq!(
            decode(&[b'A', 0x80, 0x8D, 0xA0, 0xE9]),
            Ok("A\u{2022}\u{201C}\u{20AC}\u{E9}".to_owned())
        );
    }

    #[test]
    fn unmarked_utf8_is_pdf_doc_encoding() {
        assert_eq!(decode(&[0xC3, 0xA9]), Ok("\u{C3}\u{A9}".to_owned()));
    }

    #[test]
    fn rejects_undefined_pdf_doc_encoding_bytes() {
        for byte in [0x7F, 0x9F, 0xAD] {
            assert_eq!(
                decode(&[byte]),
                Err(TextStringError::UndefinedPdfDocEncodingByte { byte })
            );
        }
    }
}
