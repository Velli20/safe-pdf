use pdf_graphics::transform::Transform;

use crate::{
    operator_trait::{OperandContext, PdfOperator},
    pdf_operator_backend::{BackendError, PdfOperatorBackend},
    variants::PdfOperatorVariant,
};
use pdf_object_reader::ReadResult;

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
    const NAME: &'static [u8] = b"Td";

    const OPERAND_COUNT: Option<usize> = Some(2);

    fn read(operands: &mut OperandContext<'_, '_>) -> ReadResult<PdfOperatorVariant> {
        let [tx, ty] = operands.read_array()?;
        Ok(PdfOperatorVariant::MoveTextPosition(Self::new(tx, ty)))
    }

    fn call<T: PdfOperatorBackend>(&self, backend: &mut T) -> Result<(), BackendError<T>> {
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
    const NAME: &'static [u8] = b"TD";

    const OPERAND_COUNT: Option<usize> = Some(2);

    fn read(operands: &mut OperandContext<'_, '_>) -> ReadResult<PdfOperatorVariant> {
        let [tx, ty] = operands.read_array()?;
        Ok(PdfOperatorVariant::MoveTextPositionAndSetLeading(
            Self::new(tx, ty),
        ))
    }

    fn call<T: PdfOperatorBackend>(&self, backend: &mut T) -> Result<(), BackendError<T>> {
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
            matrix: Transform::from(matrix),
        }
    }

    /// Returns the text matrix.
    pub const fn matrix(&self) -> &Transform {
        &self.matrix
    }
}

impl PdfOperator for SetTextMatrix {
    const NAME: &'static [u8] = b"Tm";

    const OPERAND_COUNT: Option<usize> = Some(6);

    fn read(operands: &mut OperandContext<'_, '_>) -> ReadResult<PdfOperatorVariant> {
        Ok(PdfOperatorVariant::SetTextMatrix(Self::new(
            operands.read_array()?,
        )))
    }

    fn call<T: PdfOperatorBackend>(&self, backend: &mut T) -> Result<(), BackendError<T>> {
        backend.set_text_matrix(&self.matrix)
    }
}

/// Moves to the start of the next line.
/// This has the same effect as `MoveTextPosition { tx: 0.0, ty: -leading }`,
/// where `leading` is the current value of the text leading parameter in the text state.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MoveToNextLine;

impl PdfOperator for MoveToNextLine {
    const NAME: &'static [u8] = b"T*";

    const OPERAND_COUNT: Option<usize> = Some(0);

    fn read(_operands: &mut OperandContext<'_, '_>) -> ReadResult<PdfOperatorVariant> {
        Ok(PdfOperatorVariant::MoveToNextLine(Self))
    }

    fn call<T: PdfOperatorBackend>(&self, backend: &mut T) -> Result<(), BackendError<T>> {
        backend.move_to_start_of_next_line()
    }
}
