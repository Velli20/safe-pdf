/// Big-endian `u16` code units decoded from a byte string.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BigEndianU16Units {
    /// Complete two-byte code units from the input.
    pub units: Vec<u16>,
    /// The unmatched final byte, when the input length is odd.
    pub trailing_byte: Option<u8>,
}

impl From<&[u8]> for BigEndianU16Units {
    /// Decodes complete pairs of bytes into big-endian `u16` code units.
    ///
    /// The unmatched final byte of an odd-length input is returned separately
    /// so callers can apply the malformed-input policy required by their PDF
    /// context.
    fn from(bytes: &[u8]) -> Self {
        let mut units = Vec::with_capacity(bytes.len() / 2);
        let mut chunks = bytes.chunks_exact(2);

        for pair in &mut chunks {
            if let [high, low] = pair {
                units.push(u16::from_be_bytes([*high, *low]));
            }
        }

        Self {
            units,
            trailing_byte: chunks.remainder().first().copied(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_complete_big_endian_units() {
        assert_eq!(
            BigEndianU16Units::from(&[0x00, 0x01, 0x12, 0x34][..]),
            BigEndianU16Units {
                units: vec![1, 0x1234],
                trailing_byte: None,
            }
        );
    }

    #[test]
    fn returns_an_unmatched_trailing_byte() {
        assert_eq!(
            BigEndianU16Units::from(&[0x00, 0x01, 0xFF][..]),
            BigEndianU16Units {
                units: vec![1],
                trailing_byte: Some(0xFF),
            }
        );
    }

    #[test]
    fn decodes_empty_input() {
        assert_eq!(
            BigEndianU16Units::from(&[][..]),
            BigEndianU16Units {
                units: Vec::new(),
                trailing_byte: None,
            }
        );
    }
}
