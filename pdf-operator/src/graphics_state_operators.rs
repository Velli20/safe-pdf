use crate::{error::PdfPainterError, pdf_operator::PdfOperatorVariant};

/// Sets the line width for path stroking. (PDF operator `w`)
#[derive(Debug, Clone, PartialEq)]
pub struct SetLineWidth {
    /// The new line width in user space units.
    width: f32,
}

impl SetLineWidth {
    pub const fn operator_name() -> &'static str {
        "w"
    }

    pub fn new(width: f32) -> Self {
        Self { width }
    }

    pub fn read() -> Result<PdfOperatorVariant, PdfPainterError> {
        Err(PdfPainterError::UnimplementedOperation(
            Self::operator_name(),
        ))
    }
}

/// Sets the line cap style for path stroking. (PDF operator `J`)
#[derive(Debug, Clone, PartialEq)]
pub struct SetLineCapStyle {
    /// The line cap style to apply.
    /// 0 for butt cap, 1 for round cap, 2 for projecting square cap.
    style: u8,
}

impl SetLineCapStyle {
    pub const fn operator_name() -> &'static str {
        "J"
    }

    pub fn new(style: u8) -> Self {
        Self { style }
    }

    pub fn read() -> Result<PdfOperatorVariant, PdfPainterError> {
        Err(PdfPainterError::UnimplementedOperation(
            Self::operator_name(),
        ))
    }
}

/// Sets the line join style for path stroking. (PDF operator `j`)
#[derive(Debug, Clone, PartialEq)]
pub struct SetLineJoinStyle {
    /// The line join style to apply.
    /// 0 for miter join, 1 for round join, 2 for bevel join.
    style: u8,
}

impl SetLineJoinStyle {
    pub const fn operator_name() -> &'static str {
        "j"
    }

    pub fn new(style: u8) -> Self {
        Self { style }
    }

    pub fn read() -> Result<PdfOperatorVariant, PdfPainterError> {
        Err(PdfPainterError::UnimplementedOperation(
            Self::operator_name(),
        ))
    }
}

/// Sets the miter limit for path stroking. (PDF operator `M`)
#[derive(Debug, Clone, PartialEq)]
pub struct SetMiterLimit {
    /// The new miter limit. This controls when a miter join is automatically
    /// converted to a bevel join to prevent excessively long spikes.
    limit: f32,
}

impl SetMiterLimit {
    pub const fn operator_name() -> &'static str {
        "M"
    }

    pub fn new(limit: f32) -> Self {
        Self { limit }
    }

    pub fn read() -> Result<PdfOperatorVariant, PdfPainterError> {
        Err(PdfPainterError::UnimplementedOperation(
            Self::operator_name(),
        ))
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

impl SetDashPattern {
    pub const fn operator_name() -> &'static str {
        "d"
    }

    pub fn new(array: Vec<f32>, phase: f32) -> Self {
        Self { array, phase }
    }

    pub fn read() -> Result<PdfOperatorVariant, PdfPainterError> {
        Err(PdfPainterError::UnimplementedOperation(
            Self::operator_name(),
        ))
    }
}

/// Saves the current graphics state on the graphics state stack. (PDF operator `q`)
#[derive(Debug, Clone, PartialEq)]
pub struct SaveGraphicsState;

impl SaveGraphicsState {
    pub const fn operator_name() -> &'static str {
        "q"
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

/// Restores the graphics state by removing the most recently saved state from the stack. (PDF operator `Q`)
#[derive(Debug, Clone, PartialEq)]
pub struct RestoreGraphicsState;

impl RestoreGraphicsState {
    pub const fn operator_name() -> &'static str {
        "Q"
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

/// Modifies the current transformation matrix (CTM) by concatenating the specified matrix. (PDF operator `cm`)
#[derive(Debug, Clone, PartialEq)]
pub struct ConcatMatrix {
    /// The matrix to concatenate with the CTM.
    /// Represented as `[a, b, c, d, e, f]`.
    matrix: [f32; 6],
}

impl ConcatMatrix {
    pub const fn operator_name() -> &'static str {
        "cm"
    }

    pub fn new(matrix: [f32; 6]) -> Self {
        Self { matrix }
    }

    pub fn read() -> Result<PdfOperatorVariant, PdfPainterError> {
        Err(PdfPainterError::UnimplementedOperation(
            Self::operator_name(),
        ))
    }
}
