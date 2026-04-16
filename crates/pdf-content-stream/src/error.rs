use pdf_object::error::ObjectError;
use pdf_parser::error::ParserError;
use pdf_tokenizer::error::TokenizerError;
use thiserror::Error;

/// Errors that can occur while parsing or validating PDF content stream operators.
#[derive(Error, Debug, PartialEq)]
pub enum PdfOperatorError {
    #[error("PDF operator '{0}' is recognized but not implemented")]
    UnsupportedOperator(&'static str),
    #[error("Unknown PDF operator '{0}'")]
    UnknownOperator(String),
    #[error("Missing operand; expected {expected}")]
    OperandMissing { expected: &'static str },
    #[error("Invalid operand type; expected {expected}, found {found}")]
    OperandTypeMismatch {
        expected: &'static str,
        found: &'static str,
    },
    #[error("Invalid operand value; expected {expected}, found '{found}'")]
    InvalidOperandValue {
        expected: &'static str,
        found: String,
    },
    #[error("Failed to convert operand to {target_type}: {source}")]
    OperandNumberConversion {
        target_type: &'static str,
        #[source]
        source: ObjectError,
    },
    #[error("Operator '{operator}' expects {expected} operand(s), got {actual}")]
    OperandCountMismatch {
        operator: String,
        expected: usize,
        actual: usize,
    },
    #[error("Tokenizer error while reading a content stream: {0}")]
    TokenizerError(#[from] TokenizerError),
    #[error("Parser error while reading a content stream: {0}")]
    ParserError(#[from] ParserError),
    #[error("Text-showing operator received an empty text operand")]
    EmptyTextOperand,
    #[error("Object error while reading a content stream: {0}")]
    Object(#[from] ObjectError),
}
