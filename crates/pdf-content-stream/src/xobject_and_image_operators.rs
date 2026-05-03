use pdf_image::InlineImage;
use pdf_object::object_resolver::PassthroughResolver;
use pdf_parser::parser::PdfParser;

use crate::{
    error::PdfOperatorError,
    pdf_operator::{Operands, PdfOperator, PdfOperatorVariant},
    pdf_operator_backend::{BackendError, PdfOperatorBackend},
};

/// Invokes a named XObject.
/// XObjects are external objects such as images or self-contained page descriptions (Form XObjects).
#[derive(Debug, Clone, PartialEq)]
pub struct InvokeXObject {
    /// The name of the XObject resource to invoke, as defined in the resource dictionary.
    name: String,
}

impl InvokeXObject {
    pub fn new(name: String) -> Self {
        Self { name }
    }
}

impl PdfOperator for InvokeXObject {
    const NAME: &'static [u8] = b"Do";

    const OPERAND_COUNT: Option<usize> = Some(1);

    fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        let name = operands.get_str()?;
        Ok(PdfOperatorVariant::InvokeXObject(Self::new(name)))
    }

    fn call<T: PdfOperatorBackend>(&self, backend: &mut T) -> Result<(), BackendError<T>> {
        backend.invoke_xobject(&self.name)
    }
}

impl PdfOperator for InlineImage {
    const NAME: &'static [u8] = b"BI";

    const OPERAND_COUNT: Option<usize> = Some(0);

    fn read(_operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        Err(PdfOperatorError::UnsupportedOperator("BI"))
    }

    fn parse<'a>(
        parser: &mut PdfParser<'a>,
    ) -> Result<Option<PdfOperatorVariant>, PdfOperatorError> {
        let image = parser.parse_inline_image(&PassthroughResolver)?;
        Ok(Some(PdfOperatorVariant::InlineImage(image)))
    }

    fn call<T: PdfOperatorBackend>(&self, backend: &mut T) -> Result<(), BackendError<T>> {
        backend.paint_inline_image(self)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use pdf_image::InlineImage;
    use pdf_object::dictionary::Dictionary;

    use crate::pdf_operator::PdfOperator;
    use crate::recording_pdf_operator_backend::{RecordedOperation, RecordingBackend};

    #[test]
    fn inline_image_call_dispatches_to_backend_hook() {
        let image = InlineImage::new(Dictionary::new(BTreeMap::new()), vec![0x01, 0x02]);
        let mut backend = RecordingBackend::default();

        image
            .call(&mut backend)
            .expect("inline image should dispatch");

        assert_eq!(
            backend.operations,
            vec![RecordedOperation::PaintInlineImage {
                image: image.clone()
            }]
        );
    }
}
