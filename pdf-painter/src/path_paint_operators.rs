use crate::PdfOperator;

/// Strokes the current path. (PDF operator `S`)
#[derive(Debug, Clone, PartialEq)]
pub struct StrokePath;

impl PdfOperator for StrokePath {
    fn operator() -> &'static str {
        "S"
    }
}
impl StrokePath {
    pub fn new() -> Self {
        Self
    }
}

/// Closes the current subpath and then strokes the path. (PDF operator `s`)
/// This is equivalent to a `ClosePath` followed by a `StrokePath`.
#[derive(Debug, Clone, PartialEq)]
pub struct CloseStrokePath;

impl PdfOperator for CloseStrokePath {
    fn operator() -> &'static str {
        "s"
    }
}
impl CloseStrokePath {
    pub fn new() -> Self {
        Self
    }
}

/// Fills the current path using the non-zero winding number rule. (PDF operator `f` or `F`)
/// The `F` operator is a synonym for `f`.
#[derive(Debug, Clone, PartialEq)]
pub struct FillPathNonZero;

impl PdfOperator for FillPathNonZero {
    fn operator() -> &'static str {
        "f"
    }
} // or "F"

impl FillPathNonZero {
    pub fn new() -> Self {
        Self
    }
}

/// Fills the current path using the even-odd rule. (PDF operator `f*`)
#[derive(Debug, Clone, PartialEq)]
pub struct FillPathEvenOdd;

impl PdfOperator for FillPathEvenOdd {
    fn operator() -> &'static str {
        "f*"
    }
}

impl FillPathEvenOdd {
    pub fn new() -> Self {
        Self
    }
}

/// Fills and then strokes the current path, using the non-zero winding number rule to determine the region to fill.
/// (PDF operator `B`)
#[derive(Debug, Clone, PartialEq)]
pub struct FillAndStrokePathNonZero;

impl PdfOperator for FillAndStrokePathNonZero {
    fn operator() -> &'static str {
        "B"
    }
}

impl FillAndStrokePathNonZero {
    pub fn new() -> Self {
        Self
    }
}

/// Fills and then strokes the current path, using the even-odd rule to determine the region to fill.
/// (PDF operator `B*`)
#[derive(Debug, Clone, PartialEq)]
pub struct FillAndStrokePathEvenOdd;

impl PdfOperator for FillAndStrokePathEvenOdd {
    fn operator() -> &'static str {
        "B*"
    }
}

impl FillAndStrokePathEvenOdd {
    pub fn new() -> Self {
        Self
    }
}

/// Closes, fills, and then strokes the current path, using the non-zero winding number rule to determine the region to fill.
/// (PDF operator `b`)
#[derive(Debug, Clone, PartialEq)]
pub struct CloseFillAndStrokePathNonZero;

impl PdfOperator for CloseFillAndStrokePathNonZero {
    fn operator() -> &'static str {
        "b"
    }
}

impl CloseFillAndStrokePathNonZero {
    pub fn new() -> Self {
        Self
    }
}

/// Closes, fills, and then strokes the current path, using the even-odd rule to determine the region to fill.
/// (PDF operator `b*`)
#[derive(Debug, Clone, PartialEq)]
pub struct CloseFillAndStrokePathEvenOdd;

impl PdfOperator for CloseFillAndStrokePathEvenOdd {
    fn operator() -> &'static str {
        "b*"
    }
}

impl CloseFillAndStrokePathEvenOdd {
    pub fn new() -> Self {
        Self
    }
}

/// Ends the current path object without filling or stroking it. (PDF operator `n`)
/// This operator is a path-painting no-op, used to discard the current path.
#[derive(Debug, Clone, PartialEq)]
pub struct EndPath;

impl PdfOperator for EndPath {
    fn operator() -> &'static str {
        "n"
    }
}

impl EndPath {
    pub fn new() -> Self {
        Self
    }
}
