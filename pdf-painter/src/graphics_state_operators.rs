use crate::PdfOperator;

/// Sets the line width for path stroking. (PDF operator `w`)
#[derive(Debug, Clone, PartialEq)]
pub struct SetLineWidth {
    /// The new line width in user space units.
    width: f32,
}

impl PdfOperator for SetLineWidth {
    fn operator() -> &'static str {
        "w"
    }
}

impl SetLineWidth {
    pub fn new(width: f32) -> Self {
        Self { width }
    }
}

/// Sets the line cap style for path stroking. (PDF operator `J`)
#[derive(Debug, Clone, PartialEq)]
pub struct SetLineCapStyle {
    /// The line cap style to apply.
    /// 0 for butt cap, 1 for round cap, 2 for projecting square cap.
    style: u8,
}

impl PdfOperator for SetLineCapStyle {
    fn operator() -> &'static str {
        "J"
    }
}

impl SetLineCapStyle {
    pub fn new(style: u8) -> Self {
        Self { style }
    }
}

/// Sets the line join style for path stroking. (PDF operator `j`)
#[derive(Debug, Clone, PartialEq)]
pub struct SetLineJoinStyle {
    /// The line join style to apply.
    /// 0 for miter join, 1 for round join, 2 for bevel join.
    style: u8,
}

impl PdfOperator for SetLineJoinStyle {
    fn operator() -> &'static str {
        "j"
    }
}

impl SetLineJoinStyle {
    pub fn new(style: u8) -> Self {
        Self { style }
    }
}

/// Sets the miter limit for path stroking. (PDF operator `M`)
#[derive(Debug, Clone, PartialEq)]
pub struct SetMiterLimit {
    /// The new miter limit. This controls when a miter join is automatically
    /// converted to a bevel join to prevent excessively long spikes.
    limit: f32,
}

impl PdfOperator for SetMiterLimit {
    fn operator() -> &'static str {
        "M"
    }
}

impl SetMiterLimit {
    pub fn new(limit: f32) -> Self {
        Self { limit }
    }
}

/// Sets the dash pattern for path stroking. (PDF operator `d`)
#[derive(Debug, Clone, PartialEq)]
pub struct SetDashPattern {
    /// An array of numbers specifying the lengths of alternating dashes and gaps.
    array: Vec<f32>,
    /// The phase, specifying the distance into the dash pattern at which to start.
    phase: f32,
}

impl PdfOperator for SetDashPattern {
    fn operator() -> &'static str {
        "d"
    }
}

impl SetDashPattern {
    pub fn new(array: Vec<f32>, phase: f32) -> Self {
        Self { array, phase }
    }
}

/// Saves the current graphics state on the graphics state stack. (PDF operator `q`)
#[derive(Debug, Clone, PartialEq)]
pub struct SaveGraphicsState;

impl PdfOperator for SaveGraphicsState {
    fn operator() -> &'static str {
        "q"
    }
}

impl SaveGraphicsState {
    pub fn new() -> Self {
        Self
    }
}

/// Restores the graphics state by removing the most recently saved state from the stack. (PDF operator `Q`)
#[derive(Debug, Clone, PartialEq)]
pub struct RestoreGraphicsState;

impl PdfOperator for RestoreGraphicsState {
    fn operator() -> &'static str {
        "Q"
    }
}

impl RestoreGraphicsState {
    pub fn new() -> Self {
        Self
    }
}

/// Modifies the current transformation matrix (CTM) by concatenating the specified matrix. (PDF operator `cm`)
#[derive(Debug, Clone, PartialEq)]
pub struct ConcatMatrix {
    /// The matrix to concatenate with the CTM.
    /// Represented as `[a, b, c, d, e, f]`.
    matrix: [f32; 6],
}

impl PdfOperator for ConcatMatrix {
    fn operator() -> &'static str {
        "cm"
    }
}

impl ConcatMatrix {
    pub fn new(matrix: [f32; 6]) -> Self {
        Self { matrix }
    }
}
