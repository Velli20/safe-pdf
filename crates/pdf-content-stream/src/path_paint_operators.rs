use crate::{
    error::PdfOperatorError,
    pdf_operator::{Operands, PdfOperator, PdfOperatorVariant},
    pdf_operator_backend::PdfOperatorBackend,
};

/// Strokes the current path.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StrokePath;

impl PdfOperator for StrokePath {
    const NAME: &'static str = "S";

    const OPERAND_COUNT: Option<usize> = Some(0);

    fn read(_operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        Ok(PdfOperatorVariant::StrokePath(Self))
    }

    fn call<T: PdfOperatorBackend>(&self, backend: &mut T) -> Result<(), T::ErrorType> {
        backend.stroke_path()
    }
}

/// Closes the current subpath and then strokes the path.
/// This is equivalent to a `ClosePath` followed by a `StrokePath`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CloseStrokePath;

impl PdfOperator for CloseStrokePath {
    const NAME: &'static str = "s";

    const OPERAND_COUNT: Option<usize> = Some(0);

    fn read(_operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        Ok(PdfOperatorVariant::CloseStrokePath(Self))
    }

    fn call<T: PdfOperatorBackend>(&self, backend: &mut T) -> Result<(), T::ErrorType> {
        backend.close_and_stroke_path()
    }
}

/// Fills the current path using the non-zero winding number rule.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FillPathNonZero;

impl PdfOperator for FillPathNonZero {
    const NAME: &'static str = "f"; // TODO: or "F"

    const OPERAND_COUNT: Option<usize> = Some(0);

    fn read(_operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        Ok(PdfOperatorVariant::FillPathNonZero(Self))
    }

    fn call<T: PdfOperatorBackend>(&self, backend: &mut T) -> Result<(), T::ErrorType> {
        backend.fill_path_nonzero_winding()
    }
}

/// Fills the current path using the even-odd rule.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FillPathEvenOdd;

impl PdfOperator for FillPathEvenOdd {
    const NAME: &'static str = "f*";

    const OPERAND_COUNT: Option<usize> = Some(0);

    fn read(_operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        Ok(PdfOperatorVariant::FillPathEvenOdd(Self))
    }

    fn call<T: PdfOperatorBackend>(&self, backend: &mut T) -> Result<(), T::ErrorType> {
        backend.fill_path_even_odd()
    }
}

/// Fills and then strokes the current path, using the non-zero winding number rule
/// to determine the region to fill.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FillAndStrokePathNonZero;

impl PdfOperator for FillAndStrokePathNonZero {
    const NAME: &'static str = "B";

    const OPERAND_COUNT: Option<usize> = Some(0);

    fn read(_operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        Ok(PdfOperatorVariant::FillAndStrokePathNonZero(Self))
    }

    fn call<T: PdfOperatorBackend>(&self, backend: &mut T) -> Result<(), T::ErrorType> {
        backend.fill_and_stroke_path_nonzero_winding()
    }
}

/// Fills and then strokes the current path, using the even-odd rule to determine the
/// region to fill.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FillAndStrokePathEvenOdd;

impl PdfOperator for FillAndStrokePathEvenOdd {
    const NAME: &'static str = "B*";

    const OPERAND_COUNT: Option<usize> = Some(0);

    fn read(_operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        Ok(PdfOperatorVariant::FillAndStrokePathEvenOdd(Self))
    }

    fn call<T: PdfOperatorBackend>(&self, backend: &mut T) -> Result<(), T::ErrorType> {
        backend.fill_and_stroke_path_even_odd()
    }
}

/// Closes, fills, and then strokes the current path, using the non-zero winding number
/// rule to determine the region to fill.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CloseFillAndStrokePathNonZero;

impl PdfOperator for CloseFillAndStrokePathNonZero {
    const NAME: &'static str = "b";

    const OPERAND_COUNT: Option<usize> = Some(0);

    fn read(_operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        Ok(PdfOperatorVariant::CloseFillAndStrokePathNonZero(
            Self,
        ))
    }

    fn call<T: PdfOperatorBackend>(&self, backend: &mut T) -> Result<(), T::ErrorType> {
        backend.close_fill_and_stroke_path_nonzero_winding()
    }
}

/// Closes, fills, and then strokes the current path, using the even-odd rule to determine
/// the region to fill.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CloseFillAndStrokePathEvenOdd;

impl PdfOperator for CloseFillAndStrokePathEvenOdd {
    const NAME: &'static str = "b*";

    const OPERAND_COUNT: Option<usize> = Some(0);

    fn read(_operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        Ok(PdfOperatorVariant::CloseFillAndStrokePathEvenOdd(
            Self,
        ))
    }

    fn call<T: PdfOperatorBackend>(&self, backend: &mut T) -> Result<(), T::ErrorType> {
        backend.close_fill_and_stroke_path_even_odd()
    }
}

/// Ends the current path object without filling or stroking it.
/// This operator is a path-painting no-op, used to discard the current path.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EndPath;

impl PdfOperator for EndPath {
    const NAME: &'static str = "n";

    const OPERAND_COUNT: Option<usize> = Some(0);

    fn read(_operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        Ok(PdfOperatorVariant::EndPath(Self))
    }

    fn call<T: PdfOperatorBackend>(&self, backend: &mut T) -> Result<(), T::ErrorType> {
        backend.end_path_no_op()
    }
}
