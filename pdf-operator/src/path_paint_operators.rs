use crate::{
    error::PdfPainterError,
    pdf_operator::{Operands, PdfOperatorVariant},
};

/// Strokes the current path. (PDF operator `S`)
#[derive(Debug, Clone, PartialEq)]
pub struct StrokePath;

impl StrokePath {
    pub const fn operator_name() -> &'static str {
        "S"
    }

    pub const fn new() -> Self {
        Self
    }

    pub fn read(_operands: &mut Operands) -> Result<PdfOperatorVariant, PdfPainterError> {
        Ok(PdfOperatorVariant::StrokePath(Self::new()))
    }
}

/// Closes the current subpath and then strokes the path. (PDF operator `s`)
/// This is equivalent to a `ClosePath` followed by a `StrokePath`.
#[derive(Debug, Clone, PartialEq)]
pub struct CloseStrokePath;

impl CloseStrokePath {
    pub const fn operator_name() -> &'static str {
        "s"
    }

    pub const fn new() -> Self {
        Self
    }

    pub fn read(_operands: &mut Operands) -> Result<PdfOperatorVariant, PdfPainterError> {
        Ok(PdfOperatorVariant::CloseStrokePath(Self::new()))
    }
}

/// Fills the current path using the non-zero winding number rule. (PDF operator `f` or `F`)
/// The `F` operator is a synonym for `f`.
#[derive(Debug, Clone, PartialEq)]
pub struct FillPathNonZero;

impl FillPathNonZero {
    pub const fn operator_name() -> &'static str {
        "f" // TODO: or "F"
    }

    pub const fn new() -> Self {
        Self
    }

    pub fn read(_operands: &mut Operands) -> Result<PdfOperatorVariant, PdfPainterError> {
        Ok(PdfOperatorVariant::FillPathNonZero(Self::new()))
    }
}

/// Fills the current path using the even-odd rule. (PDF operator `f*`)
#[derive(Debug, Clone, PartialEq)]
pub struct FillPathEvenOdd;

impl FillPathEvenOdd {
    pub const fn operator_name() -> &'static str {
        "f*"
    }

    pub const fn new() -> Self {
        Self
    }

    pub fn read(_operands: &mut Operands) -> Result<PdfOperatorVariant, PdfPainterError> {
        Ok(PdfOperatorVariant::FillPathEvenOdd(Self::new()))
    }
}

/// Fills and then strokes the current path, using the non-zero winding number rule to determine the region to fill.
/// (PDF operator `B`)
#[derive(Debug, Clone, PartialEq)]
pub struct FillAndStrokePathNonZero;

impl FillAndStrokePathNonZero {
    pub const fn operator_name() -> &'static str {
        "B"
    }

    pub const fn new() -> Self {
        Self
    }

    pub fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfPainterError> {
        Ok(PdfOperatorVariant::FillAndStrokePathNonZero(Self::new()))
    }
}

/// Fills and then strokes the current path, using the even-odd rule to determine the region to fill.
/// (PDF operator `B*`)
#[derive(Debug, Clone, PartialEq)]
pub struct FillAndStrokePathEvenOdd;

impl FillAndStrokePathEvenOdd {
    pub const fn operator_name() -> &'static str {
        "B*"
    }

    pub const fn new() -> Self {
        Self
    }

    pub fn read(_operands: &mut Operands) -> Result<PdfOperatorVariant, PdfPainterError> {
        Ok(PdfOperatorVariant::FillAndStrokePathEvenOdd(Self::new()))
    }
}

/// Closes, fills, and then strokes the current path, using the non-zero winding number rule to determine the region to fill.
/// (PDF operator `b`)
#[derive(Debug, Clone, PartialEq)]
pub struct CloseFillAndStrokePathNonZero;

impl CloseFillAndStrokePathNonZero {
    pub const fn operator_name() -> &'static str {
        "b"
    }

    pub const fn new() -> Self {
        Self
    }

    pub fn read(_operands: &mut Operands) -> Result<PdfOperatorVariant, PdfPainterError> {
        Ok(PdfOperatorVariant::CloseFillAndStrokePathNonZero(
            Self::new(),
        ))
    }
}

/// Closes, fills, and then strokes the current path, using the even-odd rule to determine the region to fill.
/// (PDF operator `b*`)
#[derive(Debug, Clone, PartialEq)]
pub struct CloseFillAndStrokePathEvenOdd;

impl CloseFillAndStrokePathEvenOdd {
    pub const fn operator_name() -> &'static str {
        "b*"
    }

    pub const fn new() -> Self {
        Self
    }

    pub fn read(_operands: &mut Operands) -> Result<PdfOperatorVariant, PdfPainterError> {
        Ok(PdfOperatorVariant::CloseFillAndStrokePathEvenOdd(
            Self::new(),
        ))
    }
}

/// Ends the current path object without filling or stroking it. (PDF operator `n`)
/// This operator is a path-painting no-op, used to discard the current path.
#[derive(Debug, Clone, PartialEq)]
pub struct EndPath;

impl EndPath {
    pub const fn operator_name() -> &'static str {
        "n"
    }

    pub const fn new() -> Self {
        Self
    }

    pub fn read(_operands: &mut Operands) -> Result<PdfOperatorVariant, PdfPainterError> {
        Ok(PdfOperatorVariant::EndPath(Self::new()))
    }
}
