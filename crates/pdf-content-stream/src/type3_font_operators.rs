use crate::{
    error::PdfOperatorError,
    pdf_operator::{Operands, PdfOperator, PdfOperatorVariant},
    pdf_operator_backend::PdfOperatorBackend,
};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SetCharWidthAndBoundingBox {
    wx: f32,
    wy: f32,
    llx: f32,
    lly: f32,
    urx: f32,
    ury: f32,
}

impl PdfOperator for SetCharWidthAndBoundingBox {
    const NAME: &'static str = "d1";

    const OPERAND_COUNT: Option<usize> = Some(6);

    fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        let wx = operands.get_f32()?;
        let wy = operands.get_f32()?;
        let llx = operands.get_f32()?;
        let lly = operands.get_f32()?;
        let urx = operands.get_f32()?;
        let ury = operands.get_f32()?;

        if wy != 0.0 {
            // return Err(PdfOperatorError::InvalidOperandValue {
            //     operator: Self::NAME,
            //     operand_index: 1, // wy is the second operand (index 1)
            //     expected: "0".to_string(),
            //     found: wy.to_string(),
            // });
            panic!();
        }

        Ok(PdfOperatorVariant::SetCharWidthAndBoundingBox(Self {
            wx,
            wy,
            llx,
            lly,
            urx,
            ury,
        }))
    }

    fn call<T: PdfOperatorBackend>(&self, backend: &mut T) -> Result<(), T::ErrorType> {
        backend.set_char_width_and_bounding_box(
            self.wx, self.wy, self.llx, self.lly, self.urx, self.ury,
        )
    }
}
