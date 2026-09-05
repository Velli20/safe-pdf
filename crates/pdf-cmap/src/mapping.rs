//! Shared character-code and mapping contracts for PDF CMaps.

use crate::{UnicodeSequence, WritingMode, error::CMapError};

/// Packed one-to-four-byte character code consumed from a PDF string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PdfCode {
    value: u32,
    byte_len: u8,
}

impl PdfCode {
    /// Creates a packed code after validating its byte width.
    pub fn new(value: u32, byte_len: u8) -> Result<Self, CMapError> {
        if !(1..=4).contains(&byte_len) {
            return Err(CMapError::InvalidPdfCode(
                "character codes must contain one to four bytes",
            ));
        }
        let bit_count = u32::from(byte_len)
            .checked_mul(8)
            .ok_or(CMapError::InvalidPdfCode("character code width overflowed"))?;
        let maximum = if bit_count == 32 {
            u32::MAX
        } else {
            1_u32
                .checked_shl(bit_count)
                .and_then(|number| number.checked_sub(1))
                .ok_or(CMapError::InvalidPdfCode("character code width overflowed"))?
        };
        if value > maximum {
            return Err(CMapError::InvalidPdfCode(
                "character code does not fit its declared byte length",
            ));
        }
        Ok(Self { value, byte_len })
    }

    /// Returns the packed big-endian integer value.
    #[must_use]
    pub const fn value(self) -> u32 {
        self.value
    }

    /// Returns the source length in bytes.
    #[must_use]
    pub const fn byte_len(self) -> u8 {
        self.byte_len
    }
}

/// Character identifier produced by a composite font CMap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Cid(pub u32);

/// One source-code-to-CID mapping emitted by a composite CMap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CidMapping {
    /// Source code consumed from the PDF string.
    pub source: PdfCode,
    /// Descendant-font character identifier.
    pub cid: Cid,
}

/// Object-safe decoder for a normalized Type 0 encoding CMap.
pub trait PdfCMap: Send + Sync {
    /// Decodes the first character code in `bytes`.
    fn decode_next(&self, bytes: &[u8]) -> Result<Option<CidMapping>, CMapError>;

    /// Reports whether the map uses horizontal or vertical writing.
    fn writing_mode(&self) -> WritingMode;
}

/// Object-safe mapping from PDF source codes to extraction Unicode.
pub trait ToUnicodeMap: Send + Sync {
    /// Returns all Unicode scalars represented by `code` when a mapping exists.
    fn map(&self, code: PdfCode) -> Option<UnicodeSequence>;
}
