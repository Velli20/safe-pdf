use crate::{
    error::PdfOperatorError,
    pdf_operator::{Operands, PdfOperator, PdfOperatorVariant},
};

/// Begins a text object, initializing the text matrix and text line matrix to the identity matrix.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BeginText;

impl PdfOperator for BeginText {
    const NAME: &'static str = "BT";

    const OPERAND_COUNT: usize = 0;

    fn read(_operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        Ok(PdfOperatorVariant::BeginText(Self::default()))
    }
}

/// Ends a text object, discarding the text matrix and text line matrix.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EndText;

impl PdfOperator for EndText {
    const NAME: &'static str = "ET";

    const OPERAND_COUNT: usize = 0;

    fn read(_operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        Ok(PdfOperatorVariant::EndText(Self::default()))
    }
}
