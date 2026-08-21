use crate::{
    error::PdfOperatorError,
    operands::Operands,
    operator_trait::PdfOperator,
    pdf_operator_backend::{BackendError, PdfOperatorBackend},
    variants::PdfOperatorVariant,
};

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

    fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        let [wx, wy, llx, lly, urx, ury] = operands.try_array_of::<f32, 6>()?;

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

    fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        let [wx, wy] = operands.try_array_of::<f32, 2>()?;

        Ok(PdfOperatorVariant::SetCharWidth(Self { wx, wy }))
    }

    fn call<T: PdfOperatorBackend>(&self, _backend: &mut T) -> Result<(), BackendError<T>> {
        Ok(())
    }
}
