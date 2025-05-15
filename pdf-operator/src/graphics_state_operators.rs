use crate::{
    error::PdfOperatorError,
    pdf_operator::{Operands, PdfOperator, PdfOperatorVariant},
};

/// Sets the line width for path stroking.
#[derive(Debug, Clone, PartialEq)]
pub struct SetLineWidth {
    /// The new line width in user space units.
    width: f32,
}

impl SetLineWidth {
    pub fn new(width: f32) -> Self {
        Self { width }
    }
}

impl PdfOperator for SetLineWidth {
    const NAME: &'static str = "w";

    const OPERAND_COUNT: usize = 1;

    fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        let width = operands.get_f32()?;
        Ok(PdfOperatorVariant::SetLineWidth(Self::new(width)))
    }
}

/// Sets the line cap style for path stroking.
#[derive(Debug, Clone, PartialEq)]
pub struct SetLineCapStyle {
    /// The line cap style to apply.
    /// 0 for butt cap, 1 for round cap, 2 for projecting square cap.
    style: u8,
}

impl SetLineCapStyle {
    pub fn new(style: u8) -> Self {
        Self { style }
    }
}

impl PdfOperator for SetLineCapStyle {
    const NAME: &'static str = "J";

    const OPERAND_COUNT: usize = 1;

    fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        let style = operands.get_u8()?;
        Ok(PdfOperatorVariant::SetLineCapStyle(Self::new(style)))
    }
}

/// Sets the line join style for path stroking.
#[derive(Debug, Clone, PartialEq)]
pub struct SetLineJoinStyle {
    /// The line join style to apply.
    /// 0 for miter join, 1 for round join, 2 for bevel join.
    style: u8,
}

impl SetLineJoinStyle {
    pub fn new(style: u8) -> Self {
        Self { style }
    }
}

impl PdfOperator for SetLineJoinStyle {
    const NAME: &'static str = "j";

    const OPERAND_COUNT: usize = 1;

    fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        let style = operands.get_u8()?;
        Ok(PdfOperatorVariant::SetLineJoinStyle(Self::new(style)))
    }
}

/// Sets the miter limit for path stroking.
#[derive(Debug, Clone, PartialEq)]
pub struct SetMiterLimit {
    /// The new miter limit. This controls when a miter join is automatically
    /// converted to a bevel join to prevent excessively long spikes.
    limit: f32,
}

impl SetMiterLimit {
    pub fn new(limit: f32) -> Self {
        Self { limit }
    }
}

impl PdfOperator for SetMiterLimit {
    const NAME: &'static str = "M";

    const OPERAND_COUNT: usize = 1;

    fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        let limit = operands.get_f32()?;
        Ok(PdfOperatorVariant::SetMiterLimit(Self::new(limit)))
    }
}

/// Sets the dash pattern for path stroking.
#[derive(Debug, Clone, PartialEq)]
pub struct SetDashPattern {
    /// An array of numbers specifying the lengths of alternating dashes and gaps.
    array: Vec<f32>,
    /// The phase, specifying the distance into the dash pattern at which to start.
    phase: f32,
}

impl SetDashPattern {
    pub fn new(array: Vec<f32>, phase: f32) -> Self {
        Self { array, phase }
    }
}

impl PdfOperator for SetDashPattern {
    const NAME: &'static str = "d";

    const OPERAND_COUNT: usize = 2;

    fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        let array = operands.get_f32_array()?;
        let phase = operands.get_f32()?;
        Ok(PdfOperatorVariant::SetDashPattern(Self::new(array, phase)))
    }
}

/// Saves the current graphics state on the graphics state stack.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SaveGraphicsState;

impl PdfOperator for SaveGraphicsState {
    const NAME: &'static str = "q";

    const OPERAND_COUNT: usize = 0;

    fn read(_operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        Ok(PdfOperatorVariant::SaveGraphicsState(Self::default()))
    }
}

/// Restores the graphics state by removing the most recently saved state from the stack.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RestoreGraphicsState;

impl PdfOperator for RestoreGraphicsState {
    const NAME: &'static str = "Q";

    const OPERAND_COUNT: usize = 0;

    fn read(_operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        Ok(PdfOperatorVariant::RestoreGraphicsState(Self::default()))
    }
}

/// Modifies the current transformation matrix (CTM) by concatenating the specified matrix.
#[derive(Debug, Clone, PartialEq)]
pub struct ConcatMatrix {
    /// The matrix to concatenate with the CTM.
    /// Represented as `[a, b, c, d, e, f]`.
    matrix: [f32; 6],
}

impl ConcatMatrix {
    pub fn new(matrix: [f32; 6]) -> Self {
        Self { matrix }
    }
}

impl PdfOperator for ConcatMatrix {
    const NAME: &'static str = "cm";

    const OPERAND_COUNT: usize = 6;

    fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        let a = operands.get_f32()?;
        let b = operands.get_f32()?;
        let c = operands.get_f32()?;
        let d = operands.get_f32()?;
        let e = operands.get_f32()?;
        let f = operands.get_f32()?;
        Ok(PdfOperatorVariant::ConcatMatrix(Self::new([
            a, b, c, d, e, f,
        ])))
    }
}
