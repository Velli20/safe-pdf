use crate::{
    error::PdfOperatorError,
    operands::Operands,
    operator_trait::PdfOperator,
    pdf_operator_backend::{BackendError, PdfOperatorBackend},
    variants::PdfOperatorVariant,
};

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
    const NAME: &'static [u8] = b"sh";

    const OPERAND_COUNT: Option<usize> = Some(1);

    fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        let name = operands.get_str()?;
        Ok(PdfOperatorVariant::PaintShading(Self::new(name)))
    }

    fn call<T: PdfOperatorBackend>(&self, backend: &mut T) -> Result<(), BackendError<T>> {
        backend.paint_shading(&self.name)
    }
}
