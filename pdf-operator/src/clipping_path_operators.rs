use crate::{
    error::PdfOperatorError,
    pdf_operator::{Operands, PdfOperator, PdfOperatorVariant},
    pdf_operator_backend::PdfOperatorBackend,
};

/// Modifies the current clipping path by intersecting it with the current path, using the non-zero winding number rule to determine the region to clip.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ClipNonZero;

impl PdfOperator for ClipNonZero {
    const NAME: &'static str = "W";

    const OPERAND_COUNT: usize = 0;

    fn read(_operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        Ok(PdfOperatorVariant::ClipNonZero(Self::default()))
    }

    fn call<T: PdfOperatorBackend>(&self, backend: &mut T) -> Result<(), T::ErrorType> {
        backend.clip_path_nonzero_winding()
    }
}

/// Modifies the current clipping path by intersecting it with the current path, using the even-odd rule to determine the region to clip.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ClipEvenOdd;

impl PdfOperator for ClipEvenOdd {
    const NAME: &'static str = "W*";

    const OPERAND_COUNT: usize = 0;

    fn read(_operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        Ok(PdfOperatorVariant::ClipEvenOdd(Self::default()))
    }

    fn call<T: PdfOperatorBackend>(&self, backend: &mut T) -> Result<(), T::ErrorType> {
        backend.clip_path_even_odd()
    }
}
