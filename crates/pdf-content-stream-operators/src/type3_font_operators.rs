use crate::{
    operator_trait::{OperandContext, PdfOperator},
    pdf_operator_backend::{BackendError, PdfOperatorBackend},
    variants::PdfOperatorVariant,
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

impl PdfOperator for SetCharWidthAndBoundingBox {
    const NAME: &'static [u8] = b"d1";

    const OPERAND_COUNT: Option<usize> = Some(6);

    fn read(operands: &mut OperandContext<'_, '_>) -> ReadResult<PdfOperatorVariant> {
        let [wx, wy, llx, lly, urx, ury] = operands.read_array()?;

        Ok(PdfOperatorVariant::SetCharWidthAndBoundingBox(Self {
            wx,
            wy,
            llx,
            lly,
            urx,
            ury,
        }))
    }

    fn call<T: PdfOperatorBackend>(&self, _backend: &mut T) -> Result<(), BackendError<T>> {
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

impl PdfOperator for SetCharWidth {
    const NAME: &'static [u8] = b"d0";

    const OPERAND_COUNT: Option<usize> = Some(2);

    fn read(operands: &mut OperandContext<'_, '_>) -> ReadResult<PdfOperatorVariant> {
        let [wx, wy] = operands.read_array()?;

        Ok(PdfOperatorVariant::SetCharWidth(Self { wx, wy }))
    }

    fn call<T: PdfOperatorBackend>(&self, _backend: &mut T) -> Result<(), BackendError<T>> {
        Ok(())
    }
}
