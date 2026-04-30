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

/// Begins an inline image object.
/// This operator is followed by key-value pairs defining the image's properties,
/// then the `ID` operator and image data, and finally `EI`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BeginInlineImage;

impl PdfOperator for BeginInlineImage {
    const NAME: &'static [u8] = b"BI";

    const OPERAND_COUNT: Option<usize> = Some(0);

    fn read(_operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        // The BI operator itself does not consume operands from the stack.
        // The inline image dictionary key-value pairs follow BI directly in the stream.
        // A full parser would need to enter a special state here to parse those pairs,
        // then the ID operator, then image data, then EI.
        // This function merely constructs the BeginInlineImage marker.
        Ok(PdfOperatorVariant::BeginInlineImage(Self))
    }

    fn call<T: PdfOperatorBackend>(&self, _backend: &mut T) -> Result<(), BackendError<T>> {
        Ok(())
    }
}

/// Represents the image data within an inline image object.
/// The `ID` operator itself marks the beginning of the image data stream, which is then
/// followed by the actual image data.
#[derive(Debug, Clone, PartialEq)]
pub struct InlineImageData {
    /// The raw byte data of the inline image.
    data: Vec<u8>,
}

impl InlineImageData {
    pub(crate) fn new(data: Vec<u8>) -> Self {
        Self { data }
    }

    #[cfg(test)]
    pub(crate) fn data(&self) -> &[u8] {
        &self.data
    }
}

impl PdfOperator for InlineImageData {
    const NAME: &'static [u8] = b"ID";

    const OPERAND_COUNT: Option<usize> = Some(0);

    fn read(_operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        Ok(PdfOperatorVariant::InlineImageData(Self::new(Vec::new())))
    }

    fn call<T: PdfOperatorBackend>(&self, _backend: &mut T) -> Result<(), BackendError<T>> {
        Ok(())
    }
}

/// Ends an inline image object.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EndInlineImage;

impl PdfOperator for EndInlineImage {
    const NAME: &'static [u8] = b"EI";

    const OPERAND_COUNT: Option<usize> = Some(0);

    fn read(_operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        // The EI operator does not take any operands from the stack.
        // It simply marks the end of the inline image data.
        Ok(PdfOperatorVariant::EndInlineImage(Self))
    }

    fn call<T: PdfOperatorBackend>(&self, _backend: &mut T) -> Result<(), BackendError<T>> {
        Ok(())
    }
}
