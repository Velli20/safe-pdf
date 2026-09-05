//! Error types shared by font loading, text layout, and rendering.

use thiserror::Error;

/// Failures produced while decoding or positioning PDF text.
#[derive(Debug, Error)]
pub enum TextError {
    /// Font selection or glyph lookup failed.
    #[error(transparent)]
    Font(#[from] pdf_font::FontError),
    /// CMap decoding or character-code validation failed.
    #[error(transparent)]
    CMap(#[from] pdf_cmap::error::CMapError),
    /// A PDF character code cannot be decoded by the selected encoding or CMap.
    #[error("invalid PDF character code at byte offset {offset}")]
    InvalidCharacterCode {
        /// The byte offset of the invalid source code.
        offset: usize,
    },
    /// PDF text positioning data is malformed.
    #[error("invalid PDF text positioning: {message}")]
    InvalidPositioning {
        /// A stable description of the malformed input.
        message: String,
    },
}
