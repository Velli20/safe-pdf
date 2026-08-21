use thiserror::Error;

/// Writing mode declared by a Type0 encoding CMap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WritingMode {
    /// Horizontal writing mode (`/WMode 0`).
    Horizontal,
    /// Vertical writing mode (`/WMode 1`).
    Vertical,
}

impl WritingMode {
    pub(crate) fn from_integer(value: i64) -> Self {
        if value == 1 {
            Self::Vertical
        } else {
            Self::Horizontal
        }
    }
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
