use pdf_object::error::ObjectError;
use pdf_parser::error::ParserError;
use pdf_tokenizer::error::TokenizerError;
use thiserror::Error;

/// Defines errors that can occur in pdf-painter crate.
#[derive(Error, Debug, PartialEq)]
pub enum PdfOperatorError {
    #[error("Unimplemented operation: {0}")]
    UnimplementedOperation(&'static str),

    #[error("Unknown operator: '{0}'")]
    UnknownOperator(String),

    // Error for when an operand is expected but not found (e.g., empty stack)
    #[error("Missing operand: expected a {expected_type}")]
    MissingOperand { expected_type: &'static str },

    // Error for when an operand has an unexpected type
    #[error("Invalid operand type: expected {expected_type}, found {found_type}")]
    InvalidOperandType {
        expected_type: &'static str,
        found_type: &'static str,
    },

    /// Error converting a PDF value to a number.
    #[error("Failed to convert a PDF value to number of type '{expected_type}': {source}")]
    OperandNumericConversionError {
        expected_type: &'static str,
        #[source]
        source: ObjectError,
    },

    // Error for when the number of operands is incorrect for an operator
    #[error("Incorrect operand count for operation '{op_name}': expected {expected}, got {got}")]
    IncorrectOperandCount {
        op_name: &'static str,
        expected: usize,
        got: usize,
    },

    // Errors from underlying pdf_tokenizer
    #[error("Tokenizer error: {0}")]
    Tokenizer(#[from] TokenizerError),

    #[error("Parser error: {0}")]
    Parser(#[from] ParserError),

    #[error("Empty text")]
    EmptyText,
}

// The From<TokenizerError> and From<ParserError> are now handled by #[from]
// The impl std::error::Error for PdfOperatorError is automatically provided by thiserror::Error
