use pdf_graphics::transform::Transform;

use crate::{
    error::PdfOperatorError,
    pdf_operator::{Operands, PdfOperator, PdfOperatorVariant},
    pdf_operator_backend::PdfOperatorBackend,
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

    const OPERAND_COUNT: Option<usize> = Some(2);

    fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        let tx = operands.get_f32()?;
        let ty = operands.get_f32()?;
        Ok(PdfOperatorVariant::MoveTextPosition(Self::new(tx, ty)))
    }

    fn call<T: PdfOperatorBackend>(&self, backend: &mut T) -> Result<(), T::ErrorType> {
        backend.move_text_position(self.tx, self.ty)
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

    const OPERAND_COUNT: Option<usize> = Some(2);

    fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        let tx = operands.get_f32()?;
        let ty = operands.get_f32()?;
        Ok(PdfOperatorVariant::MoveTextPositionAndSetLeading(
            Self::new(tx, ty),
        ))
    }

    fn call<T: PdfOperatorBackend>(&self, backend: &mut T) -> Result<(), T::ErrorType> {
        backend.move_text_position_and_set_leading(self.tx, self.ty)
    }
}

/// Sets the text matrix, `Tm`, and the text line matrix, `Tlm`.
#[derive(Debug, Clone, PartialEq)]
pub struct SetTextMatrix {
    /// The text matrix.
    matrix: Transform,
}

impl SetTextMatrix {
    pub fn new(matrix: [f32; 6]) -> Self {
        Self {
            matrix: Transform::from_row(
                matrix[0], matrix[1], matrix[2], matrix[3], matrix[4], matrix[5],
            ),
        }
    }
}

impl PdfOperator for SetTextMatrix {
    const NAME: &'static str = "Tm";

    const OPERAND_COUNT: Option<usize> = Some(6);

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

    fn call<T: PdfOperatorBackend>(&self, backend: &mut T) -> Result<(), T::ErrorType> {
        backend.set_text_matrix(&self.matrix)
    }
}

/// Moves to the start of the next line.
/// This has the same effect as `MoveTextPosition { tx: 0.0, ty: -leading }`,
/// where `leading` is the current value of the text leading parameter in the text state.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MoveToNextLine;

impl PdfOperator for MoveToNextLine {
    const NAME: &'static str = "T*";

    const OPERAND_COUNT: Option<usize> = Some(0);

    fn read(_operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        Ok(PdfOperatorVariant::MoveToNextLine(Self))
    }

    fn call<T: PdfOperatorBackend>(&self, backend: &mut T) -> Result<(), T::ErrorType> {
        backend.move_to_start_of_next_line()
    }
}
