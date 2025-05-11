use crate::PdfOperator;

/// Begins a new subpath by moving the current point to coordinates (x, y), omitting any connecting line segment. (PDF operator `m`)
/// If the `m` operator is the first operator in a path, it sets the current point.
#[derive(Debug, Clone, PartialEq)]
pub struct MoveTo {
    /// The x-coordinate of the new current point.
    x: f32,
    /// The y-coordinate of the new current point.
    y: f32,
}

impl PdfOperator for MoveTo {
    fn operator() -> &'static str {
        "m"
    }
}

impl MoveTo {
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// Appends a straight line segment from the current point to the specified point (x, y). (PDF operator `l`)
/// The new current point becomes (x, y).
#[derive(Debug, Clone, PartialEq)]
pub struct LineTo {
    /// The x-coordinate of the line segment's end point.
    x: f32,
    /// The y-coordinate of the line segment's end point.
    y: f32,
}

impl PdfOperator for LineTo {
    fn operator() -> &'static str {
        "l"
    }
}

impl LineTo {
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// Appends a cubic Bézier curve to the current path. (PDF operator `c`)
/// The curve extends from the current point to (x3, y3), using (x1, y1) and (x2, y2) as Bézier control points.
/// The new current point becomes (x3, y3).
#[derive(Debug, Clone, PartialEq)]
pub struct CurveTo {
    /// The x-coordinate of the first Bézier control point.
    x1: f32,
    /// The y-coordinate of the first Bézier control point.
    y1: f32,
    /// The x-coordinate of the second Bézier control point.
    x2: f32,
    /// The y-coordinate of the second Bézier control point.
    y2: f32,
    /// The x-coordinate of the curve's end point.
    x3: f32,
    /// The y-coordinate of the curve's end point.
    y3: f32,
}

impl PdfOperator for CurveTo {
    fn operator() -> &'static str {
        "c"
    }
}

impl CurveTo {
    pub fn new(x1: f32, y1: f32, x2: f32, y2: f32, x3: f32, y3: f32) -> Self {
        Self {
            x1,
            y1,
            x2,
            y2,
            x3,
            y3,
        }
    }
}

/// Appends a cubic Bézier curve to the current path. (PDF operator `v`)
/// The current point is used as the first control point (x1, y1).
/// (x2, y2) is the second Bézier control point, and (x3, y3) is the end point of the curve.
/// The new current point becomes (x3, y3).
#[derive(Debug, Clone, PartialEq)]
pub struct CurveToV {
    /// The x-coordinate of the second Bézier control point.
    x2: f32,
    /// The y-coordinate of the second Bézier control point.
    y2: f32,
    /// The x-coordinate of the curve's end point.
    x3: f32,
    /// The y-coordinate of the curve's end point.
    y3: f32,
} // Initial point replicated

impl PdfOperator for CurveToV {
    fn operator() -> &'static str {
        "v"
    }
}

impl CurveToV {
    pub fn new(x2: f32, y2: f32, x3: f32, y3: f32) -> Self {
        Self { x2, y2, x3, y3 }
    }
}

/// Appends a cubic Bézier curve to the current path. (PDF operator `y`)
/// (x1, y1) is the first Bézier control point. The second control point (x2, y2) is the same as the curve's end point (x3, y3).
/// The new current point becomes (x3, y3).
#[derive(Debug, Clone, PartialEq)]
pub struct CurveToY {
    /// The x-coordinate of the first Bézier control point.
    x1: f32,
    /// The y-coordinate of the first Bézier control point.
    y1: f32,
    /// The x-coordinate of the curve's end point (and second control point).
    x3: f32,
    /// The y-coordinate of the curve's end point (and second control point).
    y3: f32,
} // Final point replicated

impl PdfOperator for CurveToY {
    fn operator() -> &'static str {
        "y"
    }
}

impl CurveToY {
    pub fn new(x1: f32, y1: f32, x3: f32, y3: f32) -> Self {
        Self { x1, y1, x3, y3 }
    }
}

/// Closes the current subpath by appending a straight line segment from the current point
/// to the starting point of the subpath. (PDF operator `h`)
#[derive(Debug, Clone, PartialEq)]
pub struct ClosePath;

impl PdfOperator for ClosePath {
    fn operator() -> &'static str {
        "h"
    }
}

impl ClosePath {
    pub fn new() -> Self {
        Self
    }
}

/// Appends a complete rectangle, defined by its bottom-left corner (x, y), width, and height,
/// to the current path as a complete subpath. (PDF operator `re`)
/// The new current point is undefined after this operation.
#[derive(Debug, Clone, PartialEq)]
pub struct Rectangle {
    /// The x-coordinate of the bottom-left corner of the rectangle.
    x: f32,
    /// The y-coordinate of the bottom-left corner of the rectangle.
    y: f32,
    /// The width of the rectangle.
    width: f32,
    /// The height of the rectangle.
    height: f32,
}

impl PdfOperator for Rectangle {
    fn operator() -> &'static str {
        "re"
    }
}

impl Rectangle {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}
