use crate::{
    error::PdfOperatorError,
    pdf_operator::{Operands, PdfOperator, PdfOperatorVariant},
};

/// Strokes the current path.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StrokePath;

impl PdfOperator for StrokePath {
    const NAME: &'static str = "S";

    const OPERAND_COUNT: usize = 0;

    fn read(_operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        Ok(PdfOperatorVariant::StrokePath(Self::default()))
    }
}

/// Closes the current subpath and then strokes the path.
/// This is equivalent to a `ClosePath` followed by a `StrokePath`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CloseStrokePath;

impl PdfOperator for CloseStrokePath {
    const NAME: &'static str = "s";

    const OPERAND_COUNT: usize = 0;

    fn read(_operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        Ok(PdfOperatorVariant::CloseStrokePath(Self::default()))
    }
}

/// Fills the current path using the non-zero winding number rule. (PDF operator `f` or `F`)
/// The `F` operator is a synonym for `f`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FillPathNonZero;

impl PdfOperator for FillPathNonZero {
    const NAME: &'static str = "f"; // TODO: or "F"

    const OPERAND_COUNT: usize = 0;

    fn read(_operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        Ok(PdfOperatorVariant::FillPathNonZero(Self::default()))
    }
}

/// Fills the current path using the even-odd rule.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FillPathEvenOdd;

impl PdfOperator for FillPathEvenOdd {
    const NAME: &'static str = "f*";

    const OPERAND_COUNT: usize = 0;

    fn read(_operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        Ok(PdfOperatorVariant::FillPathEvenOdd(Self::default()))
    }
}

/// Fills and then strokes the current path, using the non-zero winding number rule to determine the region to fill.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FillAndStrokePathNonZero;

impl PdfOperator for FillAndStrokePathNonZero {
    const NAME: &'static str = "B";

    const OPERAND_COUNT: usize = 0;

    fn read(_operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        Ok(PdfOperatorVariant::FillAndStrokePathNonZero(Self::default()))
    }
}

/// Fills and then strokes the current path, using the even-odd rule to determine the region to fill.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FillAndStrokePathEvenOdd;

impl PdfOperator for FillAndStrokePathEvenOdd {
    const NAME: &'static str = "B*";

    const OPERAND_COUNT: usize = 0;

    fn read(_operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        Ok(PdfOperatorVariant::FillAndStrokePathEvenOdd(Self::default()))
    }
}

/// Closes, fills, and then strokes the current path, using the non-zero winding number rule to determine the region to fill.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CloseFillAndStrokePathNonZero;

impl PdfOperator for CloseFillAndStrokePathNonZero {
    const NAME: &'static str = "b";

    const OPERAND_COUNT: usize = 0;

    fn read(_operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        Ok(PdfOperatorVariant::CloseFillAndStrokePathNonZero(
            Self::default(),
        ))
    }
}

/// Closes, fills, and then strokes the current path, using the even-odd rule to determine the region to fill.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CloseFillAndStrokePathEvenOdd;

impl PdfOperator for CloseFillAndStrokePathEvenOdd {
    const NAME: &'static str = "b*";

    const OPERAND_COUNT: usize = 0;

    fn read(_operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        Ok(PdfOperatorVariant::CloseFillAndStrokePathEvenOdd(
            Self::default(),
        ))
    }
}

/// Ends the current path object without filling or stroking it.
/// This operator is a path-painting no-op, used to discard the current path.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EndPath;

impl PdfOperator for EndPath {
    const NAME: &'static str = "n";

    const OPERAND_COUNT: usize = 0;

    fn read(_operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        Ok(PdfOperatorVariant::EndPath(Self::default()))
    }
}
