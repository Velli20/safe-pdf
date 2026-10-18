use std::sync::Arc;

use crate::{
    operator_trait::{OperandContext, PdfOperator},
    pdf_operator_backend::{BackendError, PdfOperatorBackend},
    variants::PdfOperatorVariant,
};
use pdf_object_reader::{ReadResult, pdf_string::PdfString};

/// Paints the shape and color shading defined by a shading dictionary resource.
/// The `sh` operator takes one operand, the name of a shading dictionary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaintShading {
    /// The name of the shading dictionary resource from the Shading subdictionary
    /// of the current resource dictionary.
    name: Arc<[u8]>,
}

impl PaintShading {
    pub fn new(name: Arc<[u8]>) -> Self {
        Self { name }
    }
}

impl PdfOperator for PaintShading {
    const NAME: &'static [u8] = b"sh";

    const OPERAND_COUNT: Option<usize> = Some(1);

    fn read(operands: &mut OperandContext<'_, '_>) -> ReadResult<PdfOperatorVariant> {
        let name = operands.at::<PdfString>(0)?.bytes;
        Ok(PdfOperatorVariant::PaintShading(Self::new(name)))
    }

    fn call<T: PdfOperatorBackend>(&self, backend: &mut T) -> Result<(), BackendError<T>> {
        backend.paint_shading(&self.name)
    }
}
