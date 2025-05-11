use crate::{error::PdfPainterError, pdf_operator::PdfOperatorVariant};

/// Modifies the current clipping path by intersecting it with the current path, using the non-zero winding number rule to determine the region to clip.
/// (PDF operator `W`)
#[derive(Debug, Clone, PartialEq)]
pub struct ClipNonZero;

impl ClipNonZero {
    pub const fn operator_name() -> &'static str {
        "W"
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

/// Modifies the current clipping path by intersecting it with the current path, using the even-odd rule to determine the region to clip.
/// (PDF operator `W*`)
#[derive(Debug, Clone, PartialEq)]
pub struct ClipEvenOdd;

impl ClipEvenOdd {
    pub const fn operator_name() -> &'static str {
        "W*"
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
