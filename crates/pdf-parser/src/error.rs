use std::num::TryFromIntError;

use pdf_object::error::ObjectError;
use pdf_tokenizer::error::TokenizerError;
use thiserror::Error;

use crate::{
    cross_reference_table::CrossReferenceTableError, header::HeaderError,
    hex_string::HexStringError, indirect_object::IndirectObjectError,
    literal_string::LiteralStringObjectError, name::NameObjectError, number::NumberError,
    stream::StreamParsingError,
};

#[derive(Error, Debug, PartialEq)]
pub enum ParserError {
    #[error("Invalid token {0}")]
    InvalidToken(char),
    #[error("Invalid number object")]
    InvalidNumber,
    #[error("Unexpected end of file")]
    UnexpectedEndOfFile,
    #[error("Tokenizer error: {0}")]
    TokenizerError(#[from] TokenizerError),
    #[error("Cross-reference table error: {0}")]
    CrossReferenceTableError(#[from] CrossReferenceTableError),
    #[error("Hex string error: {0}")]
    HexStringError(#[from] HexStringError),
    #[error("Number error: {0}")]
    NumberError(#[from] NumberError),
    #[error("Name object error: {0}")]
    NameObjectError(#[from] NameObjectError),
    #[error("Literal string object error: {0}")]
    LiteralStringObjectError(#[from] LiteralStringObjectError),
    #[error("Header parsing error: {0}")]
    HeaderError(#[from] HeaderError),
    #[error("Error casting value: {0}")]
    ValueCastError(String),
    #[error("Error while reading a keyword. Expected '{0}' got '{1}'")]
    InvalidKeyword(String, String),
    #[error("Expected delimiter after keyword, found: {0:?}")]
    MissingDelimiterAfterKeyword(u8),
    #[error("Unexpected token '{token}' at position {position}")]
    UnexpectedTokenAt { token: String, position: usize },
    #[error("Nesting depth exceeded")]
    NestingDepthExceeded,
    #[error("Initialization error: {0}")]
    InitializationError(String),
    #[error("Missing cross-reference table; call build_xref_index() before resolving references")]
    MissingXrefTable,
    #[error("Missing cross-reference entry for object number {object_number}")]
    MissingXrefEntry { object_number: usize },
    #[error("Stream parsing error: {0}")]
    StreamParsingError(#[from] StreamParsingError),
    #[error("Object error: {0}")]
    ObjectError(#[from] ObjectError),
    #[error("Indirect object error: {0}")]
    IndirectObjectError(#[from] IndirectObjectError),
    #[error("Expected end-of-line marker (CR, LF, or CRLF)")]
    MissingEndOfLine,
}

impl From<TryFromIntError> for ParserError {
    fn from(error: TryFromIntError) -> Self {
        ParserError::ValueCastError(error.to_string())
    }
}
