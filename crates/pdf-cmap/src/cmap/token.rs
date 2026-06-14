use crate::{cmap_support::bytes_to_u32, error::CMapError};

/// Token kinds used by embedded font CMap streams.
#[derive(Debug, PartialEq)]
pub enum CMapToken {
    /// `begincmap`.
    BeginCMap,
    /// `endcmap`.
    EndCMap,
    /// `begincodespacerange`.
    BeginCodeSpaceRange,
    /// `endcodespacerange`.
    EndCodeSpaceRange,
    /// `beginbfchar`.
    BeginBfChar,
    /// `endbfchar`.
    EndBfChar,
    /// `beginbfrange`.
    BeginBfRange,
    /// `endbfrange`.
    EndBfRange,
    /// `begincidchar`.
    BeginCidChar,
    /// `endcidchar`.
    EndCidChar,
    /// `begincidrange`.
    BeginCidRange,
    /// `endcidrange`.
    EndCidRange,
    /// `def`.
    Def,
    /// `usecmap`.
    UseCMap,
    /// Any other PostScript-style operator.
    Operator(Vec<u8>),
    /// A name object without its leading `/`.
    Name(Vec<u8>),
    /// An integer literal.
    Integer(i64),
    /// A real-number literal.
    Real(f64),
    /// A PDF hex string decoded into raw bytes.
    HexString(Vec<u8>),
    /// A PDF literal string decoded into raw bytes.
    LiteralString(Vec<u8>),
    /// `<<` token.
    DoubleLeftAngleBracket,
    /// `>>` token.
    DoubleRightAngleBracket,
    /// `[` token.
    LeftSquareBracket,
    /// `]` token.
    RightSquareBracket,
}

impl CMapToken {
    /// Convert a hex-string token into a `u16`.
    pub(crate) fn u16_from_bytes(&self) -> Result<u16, CMapError> {
        let CMapToken::HexString(bytes) = self else {
            return Err(CMapError::InvalidCMapU16Bytes);
        };

        match bytes.as_slice() {
            [byte] => Ok(u16::from(*byte)),
            [first, second] => Ok(u16::from_be_bytes([*first, *second])),
            _ => Err(CMapError::InvalidCMapU16Bytes),
        }
    }

    /// Convert a hex-string token into a `u32`.
    pub(crate) fn u32_from_bytes(&self) -> Result<u32, CMapError> {
        let CMapToken::HexString(bytes) = self else {
            return Err(CMapError::InvalidCMapU32Bytes);
        };

        if bytes.len() > std::mem::size_of::<u32>() {
            return Err(CMapError::InvalidCMapU32Bytes);
        }

        Ok(bytes_to_u32(bytes))
    }

    pub(crate) fn u16_from_integer(&self) -> Result<u16, CMapError> {
        let CMapToken::Integer(value) = self else {
            return Err(CMapError::InvalidCMapU16Integer);
        };

        u16::try_from(*value).map_err(|_| CMapError::InvalidCMapU16Integer)
    }
}

#[cfg(test)]
mod tests {
    use crate::error::CMapError;

    use super::CMapToken;

    #[test]
    fn converts_one_and_two_byte_u16_hex_tokens() {
        assert_eq!(
            CMapToken::HexString(vec![0x7f]).u16_from_bytes(),
            Ok(0x007f)
        );
        assert_eq!(
            CMapToken::HexString(vec![0x12, 0x34]).u16_from_bytes(),
            Ok(0x1234)
        );
    }

    #[test]
    fn rejects_invalid_u16_tokens_and_lengths() {
        assert_eq!(
            CMapToken::HexString(Vec::new()).u16_from_bytes(),
            Err(CMapError::InvalidCMapU16Bytes)
        );
        assert_eq!(
            CMapToken::HexString(vec![0x01, 0x02, 0x03]).u16_from_bytes(),
            Err(CMapError::InvalidCMapU16Bytes)
        );
        assert_eq!(
            CMapToken::Integer(5).u16_from_bytes(),
            Err(CMapError::InvalidCMapU16Bytes)
        );
    }

    #[test]
    fn converts_empty_short_and_four_byte_u32_hex_tokens() {
        assert_eq!(CMapToken::HexString(Vec::new()).u32_from_bytes(), Ok(0));
        assert_eq!(CMapToken::HexString(vec![0x01]).u32_from_bytes(), Ok(0x01));
        assert_eq!(
            CMapToken::HexString(vec![0x01, 0x20]).u32_from_bytes(),
            Ok(0x0120)
        );
        assert_eq!(
            CMapToken::HexString(vec![0x01, 0x02, 0x03, 0x04]).u32_from_bytes(),
            Ok(0x0102_0304)
        );
    }

    #[test]
    fn rejects_overlong_and_non_hex_u32_tokens() {
        assert_eq!(
            CMapToken::HexString(vec![0x01, 0x02, 0x03, 0x04, 0x05]).u32_from_bytes(),
            Err(CMapError::InvalidCMapU32Bytes)
        );
        assert_eq!(
            CMapToken::Name(b"not-hex".to_vec()).u32_from_bytes(),
            Err(CMapError::InvalidCMapU32Bytes)
        );
    }
}
