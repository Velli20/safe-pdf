use crate::{
    error::PdfOperatorError,
    pdf_operator::{Operands, PdfOperator, PdfOperatorVariant},
    pdf_operator_backend::PdfOperatorBackend,
};

/// Begins a text object, initializing the text matrix and text line matrix to
/// the identity matrix.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BeginText;

impl PdfOperator for BeginText {
    const NAME: &'static str = "BT";

    const OPERAND_COUNT: Option<usize> = Some(0);

    fn read(_operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        Ok(PdfOperatorVariant::BeginText(Self))
    }

    fn call<T: PdfOperatorBackend>(&self, backend: &mut T) -> Result<(), T::ErrorType> {
        backend.begin_text_object()
    }
}

/// Ends a text object, discarding the text matrix and text line matrix.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EndText;

impl PdfOperator for EndText {
    const NAME: &'static str = "ET";

    const OPERAND_COUNT: Option<usize> = Some(0);

    fn read(_operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        Ok(PdfOperatorVariant::EndText(Self))
    }

    fn call<T: PdfOperatorBackend>(&self, backend: &mut T) -> Result<(), T::ErrorType> {
        backend.end_text_object()
    }
}
