use crate::{
    error::PdfOperatorError,
    pdf_operator::{Operands, PdfOperator, PdfOperatorVariant},
};

/// Moves to the start of the next line, offset from the start of the current line by (`tx`, `ty`).
/// `tx` and `ty` are numbers expressed in unscaled text space units.
/// More precisely, this operator sets the text line matrix, T_lm, to:
/// `[1 0 0 1 tx ty] * T_lm`
#[derive(Debug, Clone, PartialEq)]
pub struct MoveTextPosition {
    /// The horizontal offset.
    tx: f32,
    /// The vertical offset.
    ty: f32,
}

impl MoveTextPosition {
    pub fn new(tx: f32, ty: f32) -> Self {
        Self { tx, ty }
    }
}

impl PdfOperator for MoveTextPosition {
    const NAME: &'static str = "Td";

    const OPERAND_COUNT: usize = 2;

    fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        let tx = operands.get_f32()?;
        let ty = operands.get_f32()?;
        Ok(PdfOperatorVariant::MoveTextPosition(Self::new(tx, ty)))
    }
}

/// Moves to the start of the next line, offset from the start of the current line by (`tx`, `ty`),
/// and sets the text leading, `TL`, to `-ty`. (PDF operator `TD`)
/// This has the same effect as `SetLeading { leading: -ty }` followed by `MoveTextPosition { tx, ty }`.
#[derive(Debug, Clone, PartialEq)]
pub struct MoveTextPositionAndSetLeading {
    /// The horizontal offset.
    tx: f32,
    /// The vertical offset. The new text leading will be set to the negative of this value.
    ty: f32,
}

impl MoveTextPositionAndSetLeading {
    pub fn new(tx: f32, ty: f32) -> Self {
        Self { tx, ty }
    }
}

impl PdfOperator for MoveTextPositionAndSetLeading {
    const NAME: &'static str = "TD";

    const OPERAND_COUNT: usize = 2;

    fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        let tx = operands.get_f32()?;
        let ty = operands.get_f32()?;
        Ok(PdfOperatorVariant::MoveTextPositionAndSetLeading(
            Self::new(tx, ty),
        ))
    }
}

/// Sets the text matrix, `Tm`, and the text line matrix, `Tlm`.
/// The matrix is specified in the form `[a b c d e f]`.
#[derive(Debug, Clone, PartialEq)]
pub struct SetTextMatrix {
    /// The 6-element array representing the text matrix.
    /// `[a, b, c, d, e, f]` corresponds to the matrix:
    /// `a b 0`
    /// `c d 0`
    /// `e f 1`
    matrix: [f32; 6],
}

impl SetTextMatrix {
    pub fn new(matrix: [f32; 6]) -> Self {
        Self { matrix }
    }
}

impl PdfOperator for SetTextMatrix {
    const NAME: &'static str = "Tm";

    const OPERAND_COUNT: usize = 6;

    fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        let a = operands.get_f32()?;
        let b = operands.get_f32()?;
        let c = operands.get_f32()?;
        let d = operands.get_f32()?;
        let e = operands.get_f32()?;
        let f = operands.get_f32()?;
        Ok(PdfOperatorVariant::SetTextMatrix(Self::new([
            a, b, c, d, e, f,
        ])))
    }
}

/// Moves to the start of the next line.
/// This has the same effect as `MoveTextPosition { tx: 0.0, ty: -leading }`,
/// where `leading` is the current value of the text leading parameter in the text state.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MoveToNextLine;

impl PdfOperator for MoveToNextLine {
    const NAME: &'static str = "T*";

    const OPERAND_COUNT: usize = 0;

    fn read(_operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        Ok(PdfOperatorVariant::MoveToNextLine(Self::default()))
    }
}
