use pdf_parser::error::ParserError;
use pdf_tokenizer::error::TokenizerError;

/// Defines errors that can occur in pdf-painter crate.
#[derive(Debug, Clone, PartialEq)]
pub enum PdfPainterError {
    UnimplementedOperation(&'static str),
    OperandTokenizationError(String),
    InvalidOperandType,
}

impl std::fmt::Display for PdfPainterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PdfPainterError::UnimplementedOperation(name) => {
                write!(f, "Unimplemented operation: {}", name)
            }
            PdfPainterError::OperandTokenizationError(err) => {
                write!(f, "Operand tokenization error: {}", err)
            }
            PdfPainterError::InvalidOperandType => {
                write!(f, "Invalid operand type")
            }
        }
    }
}

impl From<TokenizerError> for PdfPainterError {
    fn from(value: TokenizerError) -> Self {
        Self::OperandTokenizationError(value.to_string())
    }
}

impl From<ParserError> for PdfPainterError {
    fn from(value: ParserError) -> Self {
        Self::OperandTokenizationError(value.to_string())
    }
}
