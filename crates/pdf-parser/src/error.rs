use pdf_filter::error::FilterError;
use pdf_object_reader::{ObjectReadError, object_error::ObjectError};
use pdf_tokenizer::error::TokenizerError;
use thiserror::Error;

use crate::{
    cross_reference_table::CrossReferenceTableError, header::HeaderError,
    literal_string::LiteralStringObjectError, name::NameObjectError,
};

#[derive(Error, Debug, PartialEq)]
pub enum ParserError {
    #[error("Invalid token {0}")]
    InvalidToken(char),
    #[error("Failed to parse number: {0}")]
    InvalidNumber(String),
    #[error("Unexpected end of file")]
    UnexpectedEndOfFile,
    #[error("Byte offset {offset} exceeds input length {input_length}")]
    InvalidOffset { offset: usize, input_length: usize },
    #[error("Tokenizer error: {0}")]
    TokenizerError(#[from] TokenizerError),
    #[error("Cross-reference table error: {0}")]
    CrossReferenceTableError(#[from] CrossReferenceTableError),
    #[error("Invalid non-hex decimal character in the input: '{0}'")]
    NotHexDecimal(char),
    #[error("Name object error: {0}")]
    NameObjectError(#[from] NameObjectError),
    #[error("Literal string object error: {0}")]
    LiteralStringObjectError(#[from] LiteralStringObjectError),
    #[error("Header parsing error: {0}")]
    HeaderError(#[from] HeaderError),
    #[error("Error while reading a keyword. Expected '{0}' got '{1}'")]
    InvalidKeyword(String, String),
    #[error(
        "Expected delimiter after keyword '{keyword}' at byte offset {position}, found: {found:?}"
    )]
    MissingDelimiterAfterKeyword {
        keyword: String,
        found: u8,
        position: usize,
    },
    #[error(
        "inline image data must start with whitespace after 'ID' at byte offset {position}, found: {found:?}"
    )]
    InlineImageMissingDataSeparator { found: u8, position: usize },
    #[error("inline image data ended unexpectedly before 'EI' terminator")]
    InlineImageMissingDataEnd,
    #[error("Unexpected token '{token}' at position {position}")]
    UnexpectedTokenAt { token: String, position: usize },
    #[error("Nesting depth exceeded")]
    NestingDepthExceeded,
    #[error("Expected an indirect object declaration at position {position}")]
    ExpectedIndirectObjectDeclaration { position: usize },
    #[error("Object error: {0}")]
    ObjectError(#[from] ObjectError),
    #[error("Stream object found without a preceding dictionary")]
    StreamObjectWithoutDictionary,
    #[error("Missing startxref marker in PDF")]
    MissingStartXref,
    #[error("Invalid cross-reference table at offset {offset}")]
    InvalidXrefAtOffset { offset: usize },
    #[error("Filter error: {0}")]
    FilterError(#[from] FilterError),
    #[error("Inline image error: {0}")]
    InlineImageError(String),
    #[error("Object read error: {0}")]
    ObjectRead(String),
}

impl From<ObjectReadError> for ParserError {
    /// Unwraps object failures raised while converting a value or loading a referenced
    /// object, so they keep their `ObjectError` variant.
    fn from(error: ObjectReadError) -> Self {
        match object_error(&error) {
            Some(object_error) => Self::ObjectError(object_error.clone()),
            None => Self::ObjectRead(error.to_string()),
        }
    }
}

/// Returns the object failure beneath a read error's location context, if any.
fn object_error(error: &ObjectReadError) -> Option<&ObjectError> {
    match error {
        ObjectReadError::At { source, .. } => object_error(source),
        ObjectReadError::Decode { source, .. } | ObjectReadError::Source { source, .. } => {
            source.downcast_ref()
        }
        _ => None,
    }
}
