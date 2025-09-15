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
    #[error("Insufficient operands: expected {expected}, found {found}")]
    InsufficientOperands { expected: usize, found: usize },
    #[error("Invalid operand count: expected {expected}, found {found}")]
    InvalidOperandCount { expected: String, found: usize },
    #[error("Operand overflow during checked arithmetic")]
    OperandOverflow,
}
