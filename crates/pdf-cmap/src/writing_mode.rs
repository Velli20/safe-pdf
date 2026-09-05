//! Writing mode for PDF text rendering.

use thiserror::Error;

/// Direction in which glyph advances are applied by the PDF font.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum WritingMode {
    /// Glyphs advance along the text-space x axis.
    #[default]
    Horizontal,
    /// Glyphs advance along the text-space y axis using vertical origins.
    Vertical,
}

/// Error returned when a CMap name does not identify a writing mode.
#[derive(Debug, Clone, Copy, Error, PartialEq, Eq)]
#[error("CMap name does not identify a writing mode")]
pub struct WritingModeNameError;

impl TryFrom<&[u8]> for WritingMode {
    type Error = WritingModeNameError;

    fn try_from(name: &[u8]) -> Result<Self, Self::Error> {
        match name {
            b"Identity-H" => Ok(Self::Horizontal),
            b"Identity-V" => Ok(Self::Vertical),
            _ => Err(WritingModeNameError),
        }
    }
}

impl From<i64> for WritingMode {
    fn from(value: i64) -> Self {
        if value == 1 {
            Self::Vertical
        } else {
            Self::Horizontal
        }
    }
}
