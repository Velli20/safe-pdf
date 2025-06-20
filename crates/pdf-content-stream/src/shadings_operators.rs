use crate::{error::PdfOperatorError, pdf_operator::{Operands, PdfOperator, PdfOperatorVariant}, pdf_operator_backend::PdfOperatorBackend};

/// Paints the shape and color shading defined by a shading dictionary resource.
/// The `sh` operator takes one operand, the name of a shading dictionary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaintShading {
    /// The name of the shading dictionary resource from the Shading subdictionary
    /// of the current resource dictionary.
    name: String,
}

impl PaintShading {
    pub fn new(name: String) -> Self {
        Self { name }
    }
}

impl PdfOperator for PaintShading {
    const NAME: &'static str = "sh";

    const OPERAND_COUNT: usize = 1;

    fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        let name = operands.get_name()?;
        Ok(PdfOperatorVariant::PaintShading(Self::new(name)))
    }

    fn call<T: PdfOperatorBackend>(&self, backend: &mut T) -> Result<(), T::ErrorType> {
        backend.paint_shading(&self.name)
    }
}