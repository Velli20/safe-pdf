use crate::{
    error::PdfPainterError,
    pdf_operator::{Operands, PdfOperatorVariant},
};

/// Begins a text object, initializing the text matrix and text line matrix to the identity matrix. (PDF operator `BT`)
#[derive(Debug, Clone, PartialEq)]
pub struct BeginText;

impl BeginText {
    pub const fn operator_name() -> &'static str {
        "BT"
    }

    pub const fn new() -> Self {
        Self
    }

    pub fn read(_operands: &mut Operands) -> Result<PdfOperatorVariant, PdfPainterError> {
        Ok(PdfOperatorVariant::BeginText(Self::new()))
    }
}

/// Ends a text object, discarding the text matrix and text line matrix. (PDF operator `ET`)
#[derive(Debug, Clone, PartialEq)]
pub struct EndText;

impl EndText {
    pub const fn operator_name() -> &'static str {
        "ET"
    }

    pub const fn new() -> Self {
        Self
    }

    pub fn read(_operands: &mut Operands) -> Result<PdfOperatorVariant, PdfPainterError> {
        Ok(PdfOperatorVariant::EndText(Self::new()))
    }
}
