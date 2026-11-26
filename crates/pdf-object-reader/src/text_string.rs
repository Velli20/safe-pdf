use crate::text_encoding::Utf16BeUnits;
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

fn decode_utf16_big_endian(bytes: &[u8]) -> Result<String, TextStringError> {
    let units = Utf16BeUnits::from(bytes);
    if units.trailing_byte().is_some() {
        return Err(TextStringError::IncompleteUtf16CodeUnit);
    }

    units
        .chars()
        .collect::<Result<String, _>>()
        .map_err(|_| TextStringError::InvalidUtf16)
}

fn decode_pdf_doc_encoding(bytes: &[u8]) -> Result<String, TextStringError> {
    bytes.iter().copied().map(decode_pdf_doc_byte).collect()
}

fn decode_pdf_doc_byte(byte: u8) -> Result<char, TextStringError> {
    let character = match byte {
        0x18..=0x1F => byte
            .checked_sub(0x18)
            .and_then(|offset| PDF_DOC_DIACRITICS.get(usize::from(offset))),
        0x80..=0x9E => byte
            .checked_sub(0x80)
            .and_then(|offset| PDF_DOC_PUNCTUATION.get(usize::from(offset))),
        0xA0 => Some(&'\u{20AC}'),
        0x7F | 0x9F | 0xAD => None,
        _ => return Ok(char::from(byte)),
    };
    character
        .copied()
        .ok_or(TextStringError::UndefinedPdfDocEncodingByte { byte })
}

/// PDFDocEncoding characters for bytes 0x18 through 0x1F.
const PDF_DOC_DIACRITICS: [char; 8] = [
    '\u{02D8}', '\u{02C7}', '\u{02C6}', '\u{02D9}', '\u{02DD}', '\u{02DB}', '\u{02DA}', '\u{02DC}',
];

/// PDFDocEncoding characters for bytes 0x80 through 0x9E.
const PDF_DOC_PUNCTUATION: [char; 31] = [
    '\u{2022}', '\u{2020}', '\u{2021}', '\u{2026}', '\u{2014}', '\u{2013}', '\u{0192}', '\u{2044}',
    '\u{2039}', '\u{203A}', '\u{2212}', '\u{2030}', '\u{201E}', '\u{201C}', '\u{201D}', '\u{2018}',
    '\u{2019}', '\u{201A}', '\u{2122}', '\u{FB01}', '\u{FB02}', '\u{0141}', '\u{0152}', '\u{0160}',
    '\u{0178}', '\u{017D}', '\u{0131}', '\u{0142}', '\u{0153}', '\u{0161}', '\u{017E}',
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_utf16_big_endian_bmp_and_supplementary_characters() {
        let bytes = [
            0xFE, 0xFF, 0x00, 0x63, 0x00, 0x61, 0x00, 0x66, 0x00, 0xE9, 0x00, 0x20, 0xD8, 0x3D,
            0xDE, 0x42,
        ];
        assert_eq!(decode(&bytes), Ok("caf\u{e9} \u{1f642}".to_owned()));
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
