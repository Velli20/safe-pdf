use std::char::DecodeUtf16Error;

/// Big-endian UTF-16 code units borrowed from a byte string.
///
/// The unmatched final byte of an odd-length input is kept separately so callers can apply the
/// malformed-input policy required by their PDF context.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Utf16BeUnits<'a> {
    pairs: &'a [[u8; 2]],
    trailing_byte: Option<u8>,
}

impl<'a> From<&'a [u8]> for Utf16BeUnits<'a> {
    /// Splits `bytes` into complete big-endian pairs without allocating.
    fn from(bytes: &'a [u8]) -> Self {
        let (pairs, remainder) = bytes.as_chunks::<2>();
        Self {
            pairs,
            trailing_byte: remainder.first().copied(),
        }
    }
}

impl Utf16BeUnits<'_> {
    /// Iterates over the complete big-endian `u16` code units.
    pub fn units(&self) -> impl Iterator<Item = u16> + '_ {
        self.pairs.iter().map(|&pair| u16::from_be_bytes(pair))
    }

    /// Decodes the code units as UTF-16, yielding an error for each unpaired surrogate.
    pub fn chars(&self) -> impl Iterator<Item = Result<char, DecodeUtf16Error>> + '_ {
        char::decode_utf16(self.units())
    }

    /// Returns the unmatched final byte, when the input length is odd.
    pub fn trailing_byte(&self) -> Option<u8> {
        self.trailing_byte
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_complete_big_endian_units() {
        let units = Utf16BeUnits::from(&[0x00, 0x01, 0x12, 0x34][..]);
        assert_eq!(units.units().collect::<Vec<_>>(), vec![1, 0x1234]);
        assert_eq!(units.trailing_byte(), None);
    }

    #[test]
    fn returns_an_unmatched_trailing_byte() {
        let units = Utf16BeUnits::from(&[0x00, 0x01, 0xFF][..]);
        assert_eq!(units.units().collect::<Vec<_>>(), vec![1]);
        assert_eq!(units.trailing_byte(), Some(0xFF));
    }

    #[test]
    fn decodes_empty_input() {
        let units = Utf16BeUnits::from(&[][..]);
        assert_eq!(units.units().count(), 0);
        assert_eq!(units.trailing_byte(), None);
    }
}
