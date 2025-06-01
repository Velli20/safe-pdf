use pdf_parser::error::ParserError;
use pdf_tokenizer::error::TokenizerError;

/// Defines errors that can occur in pdf-painter crate.
#[derive(Debug, Clone, PartialEq)]
pub enum PdfOperatorError {
    UnimplementedOperation(&'static str),
    OperandTokenizationError(String),
    InvalidOperandType,
    IncorrectOperandCount(&'static str, usize, usize),
}

impl std::fmt::Display for PdfOperatorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PdfOperatorError::UnimplementedOperation(name) => {
                write!(f, "Unimplemented operation: {}", name)
            }
            PdfOperatorError::OperandTokenizationError(err) => {
                write!(f, "Operand tokenization error: {}", err)
            }
            PdfOperatorError::InvalidOperandType => {
                write!(f, "Invalid operand type")
            }
            PdfOperatorError::IncorrectOperandCount(op, got, expected) => {
                write!(
                    f,
                    "Incorrect operand count for operation '{}' got {}, expected {}",
                    op, got, expected
                )
            }
        }
    }
}

impl From<TokenizerError> for PdfOperatorError {
    fn from(value: TokenizerError) -> Self {
        Self::OperandTokenizationError(value.to_string())
    }
}

impl From<ParserError> for PdfOperatorError {
    fn from(value: ParserError) -> Self {
        Self::OperandTokenizationError(value.to_string())
    }
}
