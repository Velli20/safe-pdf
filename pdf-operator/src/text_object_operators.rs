use crate::{error::PdfPainterError, pdf_operator::PdfOperatorVariant};

/// Begins a text object, initializing the text matrix and text line matrix to the identity matrix. (PDF operator `BT`)
#[derive(Debug, Clone, PartialEq)]
pub struct BeginText;

impl BeginText {
    pub const fn operator_name() -> &'static str {
        "BT"
    }

    pub fn new() -> Self {
        Self
    }

    pub fn read() -> Result<PdfOperatorVariant, PdfPainterError> {
        Err(PdfPainterError::UnimplementedOperation(
            Self::operator_name(),
        ))
    }
}

/// Ends a text object, discarding the text matrix and text line matrix. (PDF operator `ET`)
#[derive(Debug, Clone, PartialEq)]
pub struct EndText;

impl EndText {
    pub const fn operator_name() -> &'static str {
        "ET"
    }

    pub fn new() -> Self {
        Self
    }

    pub fn read() -> Result<PdfOperatorVariant, PdfPainterError> {
        Err(PdfPainterError::UnimplementedOperation(
            Self::operator_name(),
        ))
    }
}
