use crate::PdfOperator;

/// Modifies the current clipping path by intersecting it with the current path, using the non-zero winding number rule to determine the region to clip.
/// (PDF operator `W`)
#[derive(Debug, Clone, PartialEq)]
pub struct ClipNonZero;

impl PdfOperator for ClipNonZero {
    fn operator() -> &'static str {
        "W"
    }
}

impl ClipNonZero {
    pub fn new() -> Self {
        Self
    }
}

/// Modifies the current clipping path by intersecting it with the current path, using the even-odd rule to determine the region to clip.
/// (PDF operator `W*`)
#[derive(Debug, Clone, PartialEq)]
pub struct ClipEvenOdd;

impl PdfOperator for ClipEvenOdd {
    fn operator() -> &'static str {
        "W*"
    }
}

impl ClipEvenOdd {
    pub fn new() -> Self {
        Self
    }
}
