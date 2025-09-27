use thiserror::Error;

use crate::cff::cursor::CursorReadError;

#[derive(Debug, Error)]
pub enum CompactFontFormatError {
    #[error("Unexpected end of file: {0}")]
    UnexpectedEof(&'static str),
    #[error("Invalid data: {0}")]
    InvalidData(&'static str),
    #[error("Offsets out of range in INDEX")]
    IndexOffsetsOutOfRange,
    #[error("Invalid offsets in INDEX")]
    InvalidOffsets,
    #[error("Unexpected DICT byte: {0}")]
    UnexpectedDictByte(u8),
    #[error("Cursor read error: {0}")]
    CursorReadError(#[from] CursorReadError),
    #[error("Unsupported real number format")]
    UnsupportedRealNumber,
    #[error("{0}")]
    CharsetError(#[from] crate::cff::charset::CharsetError),
    #[error("{0}")]
    EncodingError(#[from] crate::cff::encoding::EncodingError),
    #[error("{0}")]
    TopDictReadError(#[from] crate::cff::top_dictionary_operator::TopDictReadError),
    #[error("{0}")]
    CharStringReadError(#[from] crate::cff::char_string_operator::CharStringReadError),
}
