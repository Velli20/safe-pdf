use std::sync::Arc;

use pdf_object_reader::{ReadResult, pdf_string::PdfString};

use crate::{
    OperandContext,
    pdf_operator_backend::{BackendError, PdfOperatorBackend},
};

/// Invokes a named XObject.
/// XObjects are external objects such as images or self-contained page descriptions (Form XObjects).
#[derive(Debug, Clone, PartialEq)]
pub struct InvokeXObject {
    /// The name of the XObject resource to invoke, as defined in the resource dictionary.
    name: Arc<[u8]>,
}

impl InvokeXObject {
    pub fn new(name: Arc<[u8]>) -> Self {
        Self { name }
    }

    /// Reads the operator from its operands.
    pub(crate) fn read(operands: &mut OperandContext<'_, '_>) -> ReadResult<Self> {
        let name = operands.at::<PdfString>(0)?.bytes;
        Ok(Self::new(name))
    }

    /// Dispatches the operator to `backend`.
    pub(crate) fn call<T: PdfOperatorBackend>(
        &self,
        backend: &mut T,
    ) -> Result<(), BackendError<T>> {
        backend.invoke_xobject(&self.name)
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, sync::Arc};

    use pdf_image::InlineImage;
    use pdf_object_reader::{
        ObjectReader, dictionary::Dictionary, object_resolver::PassthroughResolver,
        object_variant::ObjectVariant,
    };

    use crate::{
        recording_pdf_operator_backend::{RecordedOperation, RecordingBackend},
        variants::PdfOperatorVariant,
    };

    #[test]
    fn inline_image_call_dispatches_to_backend_hook() {
        let image = InlineImage::new(
            Dictionary::new(BTreeMap::from([
                (Vec::from(b"BPC"), ObjectVariant::Integer(8)),
                (
                    Vec::from(b"CS"),
                    pdf_object_reader::pdf_string::PdfString::from(
                        b"G",
                        pdf_object_reader::string_kind::StringKind::Name,
                    ),
                ),
                (Vec::from(b"H"), ObjectVariant::Integer(1)),
                (Vec::from(b"W"), ObjectVariant::Integer(2)),
            ])),
            vec![0x01, 0x02],
            &mut ObjectReader::new(PassthroughResolver).session(),
        )
        .expect("unfiltered inline image should be constructed");
        let image = Arc::new(image);
        let mut backend = RecordingBackend::default();

        PdfOperatorVariant::InlineImage(Arc::clone(&image))
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
