use crate::{
    OperandContext,
    pdf_operator_backend::{BackendError, PdfOperatorBackend},
};
use pdf_object_reader::ReadResult;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SetCharWidthAndBoundingBox {
    /// The x-component of the character width vector.
    pub wx: f32,
    /// The y-component of the character width vector.
    wy: f32,
    /// The x-coordinate of the lower-left corner of the character bounding box.
    llx: f32,
    /// The y-coordinate of the lower-left corner of the character bounding box.
    lly: f32,
    /// The x-coordinate of the upper-right corner of the character bounding box.
    urx: f32,
    /// The y-coordinate of the upper-right corner of the character bounding box.
    ury: f32,
}

impl SetCharWidthAndBoundingBox {
    /// Reads the operator from its operands.
    pub(crate) fn read(operands: &mut OperandContext<'_, '_>) -> ReadResult<Self> {
        let [wx, wy, llx, lly, urx, ury] = operands.read_array()?;

        Ok(Self {
            wx,
            wy,
            llx,
            lly,
            urx,
            ury,
        })
    }

    /// Dispatches the operator to `backend`.
    pub(crate) fn call<T: PdfOperatorBackend>(
        &self,
        _backend: &mut T,
    ) -> Result<(), BackendError<T>> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SetCharWidth {
    /// The x-component of the character width vector.
    pub wx: f32,
    /// The y-component of the character width vector.
    wy: f32,
}

impl SetCharWidth {
    /// Reads the operator from its operands.
    pub(crate) fn read(operands: &mut OperandContext<'_, '_>) -> ReadResult<Self> {
        let [wx, wy] = operands.read_array()?;

        Ok(Self { wx, wy })
    }

    /// Dispatches the operator to `backend`.
    pub(crate) fn call<T: PdfOperatorBackend>(
        &self,
        _backend: &mut T,
    ) -> Result<(), BackendError<T>> {
        Ok(())
    }
}
