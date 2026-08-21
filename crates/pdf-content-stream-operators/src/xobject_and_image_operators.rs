use std::rc::Rc;

use pdf_image::InlineImage;
use pdf_object::object_resolver::PassthroughResolver;
use pdf_parser::parser::PdfParser;

use crate::{
    error::PdfOperatorError,
    operands::Operands,
    operator_trait::PdfOperator,
    pdf_operator_backend::{BackendError, PdfOperatorBackend},
    variants::PdfOperatorVariant,
};

/// Invokes a named XObject.
/// XObjects are external objects such as images or self-contained page descriptions (Form XObjects).
#[derive(Debug, Clone, PartialEq)]
pub struct InvokeXObject {
    /// The name of the XObject resource to invoke, as defined in the resource dictionary.
    name: Vec<u8>,
}

impl InvokeXObject {
    pub fn new(name: Vec<u8>) -> Self {
        Self { name }
    }
}

impl PdfOperator for InvokeXObject {
    const NAME: &'static [u8] = b"Do";

    const OPERAND_COUNT: Option<usize> = Some(1);

    fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        let name = operands.get_name_bytes()?;
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
        Ok(Some(PdfOperatorVariant::InlineImage(Rc::new(image))))
    }

    fn call<T: PdfOperatorBackend>(&self, backend: &mut T) -> Result<(), BackendError<T>> {
        backend.paint_inline_image(self)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use pdf_image::InlineImage;
    use pdf_object::{
        dictionary::Dictionary, object_resolver::PassthroughResolver, object_variant::ObjectVariant,
    };

    use crate::{
        operator_trait::PdfOperator,
        recording_pdf_operator_backend::{RecordedOperation, RecordingBackend},
    };

    #[test]
    fn inline_image_call_dispatches_to_backend_hook() {
        let image = InlineImage::new(
            Dictionary::new(BTreeMap::from([
                (Vec::from(b"BPC"), ObjectVariant::Integer(8)),
                (Vec::from(b"CS"), ObjectVariant::Name(b"G".to_vec())),
                (Vec::from(b"H"), ObjectVariant::Integer(1)),
                (Vec::from(b"W"), ObjectVariant::Integer(2)),
            ])),
            vec![0x01, 0x02],
            &PassthroughResolver,
        )
        .expect("unfiltered inline image should be constructed");
        let mut backend = RecordingBackend::default();

        image
            .call(&mut backend)
            .expect("inline image should dispatch");

        assert_eq!(
            backend.operations,
            vec![RecordedOperation::PaintInlineImage {
                data: image.shared_data()
            }]
        );
    }
}
