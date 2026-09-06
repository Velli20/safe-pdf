use pdf_object_reader::text_encoding::BigEndianU16Units;

use crate::cmap_support::bytes_to_u32;

/// Convert a ToUnicode source hex string into the parser's `u16` character code.
///
/// PDF CMap source codes are variable-width byte strings. This crate stores
/// ToUnicode source codes as `u16`, so empty input maps to `.notdef` (`0`),
/// one-byte input maps directly, two-byte input is big-endian, and longer
/// input keeps the final two bytes.
pub(super) fn bytes_to_char_code(bytes: &[u8]) -> u16 {
    match bytes {
        [] => 0,
        [byte] => u16::from(*byte),
        [hi, lo] => u16::from_be_bytes([*hi, *lo]),
        _ => trailing_char_code(bytes),
    }
}

/// Decode a PDF UTF-16BE destination byte string into Unicode characters.
///
/// ToUnicode destinations are encoded as big-endian UTF-16 code units. This is
/// intentionally lossy for malformed PDF input: odd trailing bytes, unpaired
/// low surrogates, and invalid surrogate pairs are skipped instead of failing
/// the whole CMap parse.
pub(super) fn utf16_bytes_to_chars(bytes: &[u8]) -> Vec<char> {
    let mut chars = Vec::new();
    let mut units = BigEndianU16Units::from(bytes).units.into_iter();

    while let Some(unit) = units.next() {
        if is_high_surrogate(unit) {
            if let Some(low) = units.next()
                && let Some(c) = surrogate_pair_to_char(unit, low)
            {
                chars.push(c);
            }
        } else if let Some(c) = bmp_code_unit_to_char(unit) {
            chars.push(c);
        }
    }

    chars
}

/// Decode a UTF-16BE byte string and discard empty results.
///
/// Callers use this when an empty Unicode mapping should be treated the same
/// as a malformed mapping and therefore omitted from the final map.
pub(super) fn utf16_bytes_to_chars_non_empty(bytes: &[u8]) -> Option<Vec<char>> {
    let chars = utf16_bytes_to_chars(bytes);
    if chars.is_empty() { None } else { Some(chars) }
}

/// Convert a Unicode scalar value to the map value representation.
///
/// Invalid scalar values, including surrogate code points and values above the
/// Unicode range, produce an empty vector so malformed sequential ranges can be
/// skipped by the caller.
pub(super) fn codepoint_to_chars(cp: u32) -> Vec<char> {
    match char::from_u32(cp) {
        Some(c) => vec![c],
        None => Vec::new(),
    }
}

/// Convert a sequential `bfrange` destination byte string to its numeric base.
///
/// Sequential range destinations are incremented as integers after each source
/// code, so this delegates to the shared big-endian byte packing helper.
pub(super) fn sequential_base_code(bytes: &[u8]) -> u32 {
    bytes_to_u32(bytes)
}

/// Return the final two bytes of an overlong source code as a `u16`.
fn trailing_char_code(bytes: &[u8]) -> u16 {
    const PDF_CHAR_CODE_BYTES: usize = 2;

    let mut trailing = bytes.iter().rev().take(PDF_CHAR_CODE_BYTES);
    let Some(lo) = trailing.next().copied() else {
        return 0;
    };
    let Some(hi) = trailing.next().copied() else {
        return u16::from(lo);
    };

    u16::from_be_bytes([hi, lo])
}

/// Return whether `unit` is a UTF-16 high surrogate.
fn is_high_surrogate(unit: u16) -> bool {
    const HIGH_SURROGATE_END: u16 = 0xDBFF;
    const HIGH_SURROGATE_START: u16 = 0xD800;

    (HIGH_SURROGATE_START..=HIGH_SURROGATE_END).contains(&unit)
}

/// Return whether `unit` is a UTF-16 low surrogate.
fn is_low_surrogate(unit: u16) -> bool {
    const LOW_SURROGATE_START: u16 = 0xDC00;
    const LOW_SURROGATE_END: u16 = 0xDFFF;

    (LOW_SURROGATE_START..=LOW_SURROGATE_END).contains(&unit)
}

/// Decode one non-surrogate BMP code unit to a character.
///
/// Low surrogates are invalid unless consumed after a high surrogate, so they
/// are ignored here.
fn bmp_code_unit_to_char(unit: u16) -> Option<char> {
    if is_low_surrogate(unit) {
        None
    } else {
        char::from_u32(u32::from(unit))
    }
}

/// Decode a valid UTF-16 surrogate pair into one Unicode character.
fn surrogate_pair_to_char(high: u16, low: u16) -> Option<char> {
    if !is_low_surrogate(low) {
        return None;
    }

    char::from_u32(surrogate_pair_to_codepoint(high, low)?)
}

/// Convert a known-valid UTF-16 surrogate pair into its scalar value.
///
/// The checked additions are defensive: validated surrogate payloads cannot
/// exceed the Unicode range, but the workspace lints require arithmetic to make
/// overflow behavior explicit.
fn surrogate_pair_to_codepoint(high: u16, low: u16) -> Option<u32> {
    const SURROGATE_HIGH_PAYLOAD_SHIFT: u32 = 10;
    const SUPPLEMENTARY_PLANE_BASE: u32 = 0x1_0000;
    const SURROGATE_PAYLOAD_MASK: u16 = 0x03FF;

    let high_bits = u32::from(high & SURROGATE_PAYLOAD_MASK) << SURROGATE_HIGH_PAYLOAD_SHIFT;
    let low_bits = u32::from(low & SURROGATE_PAYLOAD_MASK);

    SUPPLEMENTARY_PLANE_BASE
        .checked_add(high_bits)?
        .checked_add(low_bits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packs_pdf_character_codes() {
        assert_eq!(bytes_to_char_code(&[]), 0);
        assert_eq!(bytes_to_char_code(&[0x41]), 0x41);
        assert_eq!(bytes_to_char_code(&[0x12, 0x34]), 0x1234);
        assert_eq!(bytes_to_char_code(&[0xAA, 0xBB, 0xCC]), 0xBBCC);
    }

    #[test]
    fn decodes_bmp_utf16be_units() {
        assert_eq!(utf16_bytes_to_chars(&[0x00, 0x41]), vec!['A']);
    }

    #[test]
    fn decodes_valid_surrogate_pairs() {
        assert_eq!(
            utf16_bytes_to_chars(&[0xD8, 0x3D, 0xDE, 0x00])
                .first()
                .copied(),
            char::from_u32(0x1F600)
        );
    }

    #[test]
    fn skips_unpaired_high_surrogates() {
        assert!(utf16_bytes_to_chars(&[0xD8, 0x3D]).is_empty());
        assert!(utf16_bytes_to_chars(&[0xD8, 0x3D, 0x00, 0x41]).is_empty());
    }

    #[test]
    fn skips_unpaired_low_surrogates() {
        assert!(utf16_bytes_to_chars(&[0xDE, 0x00]).is_empty());
    }

    #[test]
    fn ignores_odd_trailing_utf16_byte() {
        assert_eq!(utf16_bytes_to_chars(&[0x00, 0x41, 0x00]), vec!['A']);
    }

    #[test]
    fn rejects_invalid_scalar_values() {
        assert!(codepoint_to_chars(0x11_0000).is_empty());
        assert!(codepoint_to_chars(0xD800_u32).is_empty());
    }

    #[test]
    fn returns_none_for_empty_utf16_mapping() {
        assert_eq!(utf16_bytes_to_chars_non_empty(&[0xDE, 0x00]), None);
    }
}
