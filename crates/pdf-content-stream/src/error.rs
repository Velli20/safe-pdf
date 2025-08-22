use pdf_object::error::ObjectError;
use pdf_parser::{comment::CommentError, error::ParserError};
use pdf_tokenizer::error::TokenizerError;
use thiserror::Error;

/// Defines errors that can occur in pdf-painter crate.
#[derive(Error, Debug, PartialEq)]
pub enum PdfOperatorError {
    #[error("Unimplemented operation: {0}")]
    UnimplementedOperation(&'static str),
    #[error("Unknown operator: '{0}'")]
    UnknownOperator(String),
    #[error("Missing operand: expected a {expected_type}")]
    MissingOperand { expected_type: &'static str },
    #[error("Invalid operand type: expected {expected_type}, found {found_type}")]
    InvalidOperandType {
        expected_type: &'static str,
        found_type: &'static str,
    },
    #[error("Invalid operand value: expected {expected}, found {value}")]
    InvalidOperandValue {
        expected: &'static str,
        value: String,
    },
    #[error("Failed to convert a PDF value to number of type '{expected_type}': {source}")]
    OperandNumericConversionError {
        expected_type: &'static str,
        #[source]
        source: ObjectError,
    },
    #[error("Incorrect operand count for operation '{op_name}': expected {expected}, got {got}")]
    IncorrectOperandCount {
        op_name: &'static str,
        expected: usize,
        got: usize,
    },
    #[error("Tokenizer error: {0}")]
    Tokenizer(#[from] TokenizerError),
    #[error("Parser error: {0}")]
    Parser(#[from] ParserError),
    #[error("Empty text")]
    EmptyText,
    #[error("Comment parse error: {0}")]
    Comment(#[from] CommentError),
}
