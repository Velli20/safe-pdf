use std::iter;

use pdf_object_reader::text_encoding::Utf16BeUnits;

use crate::UnicodeSequence;

/// Decode a PDF UTF-16BE destination byte string into Unicode scalars.
///
/// ToUnicode destinations are encoded as big-endian UTF-16 code units. This is
/// intentionally lossy for malformed PDF input: an odd trailing byte and
/// unpaired surrogates are skipped instead of failing the whole CMap parse.
pub(super) fn utf16_destination(bytes: &[u8]) -> UnicodeSequence {
    Utf16BeUnits::from(bytes)
        .chars()
        .filter_map(Result::ok)
        .collect()
}

/// UTF-16 destination of a sequential `bfrange`, whose final code unit advances
/// by one for each source code in the range.
pub(super) struct SequentialDestination {
    units: Vec<u16>,
}

impl From<&[u8]> for SequentialDestination {
    /// Split the base destination into code units.
    ///
    /// A single-byte destination is accepted as one code unit, since some
    /// producers write `<41>` rather than `<0041>` for range bases.
    fn from(bytes: &[u8]) -> Self {
        let decoded = Utf16BeUnits::from(bytes);
        let mut units: Vec<u16> = decoded.units().collect();
        if units.is_empty()
            && let Some(byte) = decoded.trailing_byte()
        {
            units.push(u16::from(byte));
        }
        Self { units }
    }
}

impl SequentialDestination {
    /// Return the destination for the source code `offset` places after the
    /// range start, or `None` once the final code unit would overflow.
    pub(super) fn at(&self, offset: u16) -> Option<UnicodeSequence> {
        let (last, prefix) = self.units.split_last()?;
        let last = last.checked_add(offset)?;
        Some(
            char::decode_utf16(prefix.iter().copied().chain(iter::once(last)))
                .filter_map(Result::ok)
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chars(sequence: &UnicodeSequence) -> &[char] {
        sequence.as_slice()
    }

    #[test]
    fn decodes_bmp_utf16be_units() {
        assert_eq!(chars(&utf16_destination(&[0x00, 0x41])), ['A']);
    }

    #[test]
    fn decodes_valid_surrogate_pairs() {
        assert_eq!(
            chars(&utf16_destination(&[0xD8, 0x3D, 0xDE, 0x00]))
                .first()
                .copied(),
            char::from_u32(0x1F600)
        );
    }

    #[test]
    fn skips_unpaired_high_surrogates() {
        assert!(utf16_destination(&[0xD8, 0x3D]).as_slice().is_empty());
        assert_eq!(chars(&utf16_destination(&[0xD8, 0x3D, 0x00, 0x41])), ['A']);
    }

    #[test]
    fn skips_unpaired_low_surrogates() {
        assert!(utf16_destination(&[0xDE, 0x00]).as_slice().is_empty());
    }

    #[test]
    fn ignores_odd_trailing_utf16_byte() {
        assert_eq!(chars(&utf16_destination(&[0x00, 0x41, 0x00])), ['A']);
    }
}
