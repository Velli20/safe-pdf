use std::num::TryFromIntError;

use pdf_tokenizer::error::TokenizerError;

use crate::{
    array::ArrayError, cross_reference_table::CrossReferenceTableError,
    dictionary::DictionaryError, hex_string::HexStringError, indirect_object::IndirectObjectError,
    literal_string::LiteralStringObjectError, name::NameObjectError, stream::StreamParsingError,
    trailer::TrailerError,
};

#[derive(Debug, PartialEq)]
pub enum ParserError {
    InvalidHeader,
    InvalidTrailer,
    InvalidToken,
    StreamObjectWithoutDictionary,
    InvalidNumber,
    UnexpectedEndOfFile,
    TokenizerError(TokenizerError),
    StreamParsingError(StreamParsingError),
    CrossReferenceTableError(CrossReferenceTableError),
    HexStringError(HexStringError),
    TrailerError(TrailerError),
    ArrayError(ArrayError),
    DictionaryError(DictionaryError),
    IndirectObjectError(IndirectObjectError),
    NameObjectError(NameObjectError),
    LiteralStringObjectError(LiteralStringObjectError),
    ValueCastError(String),
    InvalidKeyword(String, String),
    MissingDelimiterAfterKeyword(u8),
}

impl From<TokenizerError> for ParserError {
    fn from(error: TokenizerError) -> Self {
        ParserError::TokenizerError(error)
    }
}

impl From<CrossReferenceTableError> for ParserError {
    fn from(error: CrossReferenceTableError) -> Self {
        ParserError::CrossReferenceTableError(error)
    }
}

impl From<DictionaryError> for ParserError {
    fn from(error: DictionaryError) -> Self {
        ParserError::DictionaryError(error)
    }
}

impl From<HexStringError> for ParserError {
    fn from(error: HexStringError) -> Self {
        ParserError::HexStringError(error)
    }
}

impl From<TrailerError> for ParserError {
    fn from(error: TrailerError) -> Self {
        ParserError::TrailerError(error)
    }
}

impl From<IndirectObjectError> for ParserError {
    fn from(error: IndirectObjectError) -> Self {
        ParserError::IndirectObjectError(error)
    }
}

impl From<NameObjectError> for ParserError {
    fn from(error: NameObjectError) -> Self {
        ParserError::NameObjectError(error)
    }
}

impl From<LiteralStringObjectError> for ParserError {
    fn from(error: LiteralStringObjectError) -> Self {
        ParserError::LiteralStringObjectError(error)
    }
}

impl From<StreamParsingError> for ParserError {
    fn from(error: StreamParsingError) -> Self {
        ParserError::StreamParsingError(error)
    }
}

impl From<TryFromIntError> for ParserError {
    fn from(error: TryFromIntError) -> Self {
        ParserError::ValueCastError(error.to_string())
    }
}

impl std::fmt::Display for ParserError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParserError::StreamObjectWithoutDictionary => {
                write!(f, "Stream object without dictionary")
            }
            ParserError::InvalidHeader => write!(f, "Invalid PDF header"),
            ParserError::CrossReferenceTableError(e) => {
                write!(f, "Error while parsing cross-reference table: {}", e)
            }
            ParserError::TrailerError(e) => {
                write!(f, "Error while parsing trailer: {}", e)
            }
            ParserError::DictionaryError(e) => {
                write!(f, "Error while parsing dictionary: {}", e)
            }
            ParserError::IndirectObjectError(e) => {
                write!(f, "Error while parsing indirect object: {}", e)
            }
            ParserError::InvalidTrailer => write!(f, "Invalid PDF trailer"),
            ParserError::InvalidToken => write!(f, "Invalid token"),
            ParserError::InvalidNumber => write!(f, "Invalid number object"),
            ParserError::MissingDelimiterAfterKeyword(d) => {
                write!(f, "Expected delimiter after keyword, found: {:?}", d)
            }
            ParserError::UnexpectedEndOfFile => write!(f, "Unexpected end of file"),
            ParserError::TokenizerError(tokenizer_error) => tokenizer_error.fmt(f),
            ParserError::StreamParsingError(message) => {
                write!(f, "Error while parsing stream: {}", message)
            }
            ParserError::HexStringError(message) => {
                write!(f, "Error while parsing hex string: {}", message)
            }
            ParserError::ArrayError(message) => {
                write!(f, "Error while parsing an array object: {}", message)
            }
            ParserError::NameObjectError(message) => {
                write!(f, "Error while name object: {}", message.to_string())
            }
            ParserError::LiteralStringObjectError(message) => {
                write!(
                    f,
                    "Error while literal string object: {}",
                    message.to_string()
                )
            }
            ParserError::ValueCastError(message) => {
                write!(f, "Error casting value: {}", message)
            }
            ParserError::InvalidKeyword(expected, actual) => {
                write!(
                    f,
                    "Error while reading a keyword. Expected '{}' got '{}'",
                    expected, actual
                )
            }
        }
    }
}
