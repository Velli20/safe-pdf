use crate::{
    error::PdfPainterError,
    pdf_operator::{Operands, PdfOperatorVariant},
};

/// Moves to the start of the next line, offset from the start of the current line by (`tx`, `ty`). (PDF operator `Td`)
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
    pub const fn operator_name() -> &'static str {
        "Td"
    }

    pub fn new(tx: f32, ty: f32) -> Self {
        Self { tx, ty }
    }

    pub fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfPainterError> {
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
    pub const fn operator_name() -> &'static str {
        "TD"
    }

    pub fn new(tx: f32, ty: f32) -> Self {
        Self { tx, ty }
    }

    pub fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfPainterError> {
        let tx = operands.get_f32()?;
        let ty = operands.get_f32()?;
        Ok(PdfOperatorVariant::MoveTextPositionAndSetLeading(
            Self::new(tx, ty),
        ))
    }
}

/// Sets the text matrix, `Tm`, and the text line matrix, `Tlm`. (PDF operator `Tm`)
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
    pub const fn operator_name() -> &'static str {
        "Tm"
    }

    pub fn new(matrix: [f32; 6]) -> Self {
        Self { matrix }
    }

    pub fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfPainterError> {
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

/// Moves to the start of the next line. (PDF operator `T*`)
/// This has the same effect as `MoveTextPosition { tx: 0.0, ty: -leading }`,
/// where `leading` is the current value of the text leading parameter in the text state.
#[derive(Debug, Clone, PartialEq)]
pub struct MoveToNextLine;

impl MoveToNextLine {
    pub const fn operator_name() -> &'static str {
        "T*"
    }

    pub const fn new() -> Self {
        Self
    }

    pub fn read(_operands: &mut Operands) -> Result<PdfOperatorVariant, PdfPainterError> {
        Ok(PdfOperatorVariant::MoveToNextLine(Self::new()))
    }
}
