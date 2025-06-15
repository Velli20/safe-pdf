use std::num::TryFromIntError;

use pdf_tokenizer::error::TokenizerError;
use thiserror::Error;

use crate::{
    array::ArrayError, boolean::BooleanError, comment::CommentError,
    cross_reference_table::CrossReferenceTableError, dictionary::DictionaryError,
    header::HeaderError, hex_string::HexStringError, literal_string::LiteralStringObjectError,
    name::NameObjectError, number::NumberError,
};

#[derive(Error, Debug, PartialEq)]
pub enum ParserError {
    #[error("Invalid token")]
    InvalidToken,

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
    #[error("Array error: {0}")]
    ArrayError(#[from] ArrayError),
    #[error("Number error: {0}")]
    NumberError(#[from] NumberError),
    #[error("Boolean error: {0}")]
    BooleanError(#[from] BooleanError),
    #[error("Dictionary error: {0}")]
    DictionaryError(#[from] DictionaryError),
    #[error("Name object error: {0}")]
    NameObjectError(#[from] NameObjectError),
    #[error("Error while parsing Comment: {0}")]
    CommentError(#[from] CommentError),
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
}

impl From<TryFromIntError> for ParserError {
    fn from(error: TryFromIntError) -> Self {
        ParserError::ValueCastError(error.to_string())
    }
}
