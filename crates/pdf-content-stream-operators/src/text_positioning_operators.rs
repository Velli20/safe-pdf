use pdf_graphics::transform::Transform;

use crate::{
    OperandContext,
    pdf_operator_backend::{BackendError, PdfOperatorBackend},
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

    /// Reads the operator from its operands.
    pub(crate) fn read(operands: &mut OperandContext<'_, '_>) -> ReadResult<Self> {
        let [tx, ty] = operands.read_array()?;
        Ok(Self::new(tx, ty))
    }

    /// Dispatches the operator to `backend`.
    pub(crate) fn call<T: PdfOperatorBackend>(
        &self,
        backend: &mut T,
    ) -> Result<(), BackendError<T>> {
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

    /// Reads the operator from its operands.
    pub(crate) fn read(operands: &mut OperandContext<'_, '_>) -> ReadResult<Self> {
        let [tx, ty] = operands.read_array()?;
        Ok(Self::new(tx, ty))
    }

    /// Dispatches the operator to `backend`.
    pub(crate) fn call<T: PdfOperatorBackend>(
        &self,
        backend: &mut T,
    ) -> Result<(), BackendError<T>> {
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

    /// Reads the operator from its operands.
    pub(crate) fn read(operands: &mut OperandContext<'_, '_>) -> ReadResult<Self> {
        Ok(Self::new(operands.read_array()?))
    }

    /// Dispatches the operator to `backend`.
    pub(crate) fn call<T: PdfOperatorBackend>(
        &self,
        backend: &mut T,
    ) -> Result<(), BackendError<T>> {
        backend.set_text_matrix(&self.matrix)
    }
}
