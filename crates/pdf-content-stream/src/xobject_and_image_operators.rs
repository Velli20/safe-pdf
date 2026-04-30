use std::collections::BTreeMap;

use pdf_object::dictionary::Dictionary;
#[cfg(test)]
use pdf_object::object_variant::ObjectVariant;

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

/// Represents a complete inline image object.
/// This operator is followed by key-value pairs defining the image's properties,
/// then the `ID` operator and image data, and finally `EI`.
#[derive(Debug, Clone, PartialEq)]
pub struct InlineImage {
    /// The key-value pairs declared after `BI` and before `ID`.
    dictionary: Dictionary,
    /// The raw byte data of the inline image.
    data: Vec<u8>,
}

impl InlineImage {
    pub(crate) fn new(dictionary: Dictionary, data: Vec<u8>) -> Self {
        Self { dictionary, data }
    }

    #[cfg(test)]
    pub(crate) fn dictionary(&self) -> &BTreeMap<String, ObjectVariant> {
        &self.dictionary.dictionary
    }

    #[cfg(test)]
    pub(crate) fn data(&self) -> &[u8] {
        &self.data
    }
}

impl PdfOperator for InlineImage {
    const NAME: &'static [u8] = b"BI";

    const OPERAND_COUNT: Option<usize> = Some(0);

    fn read(_operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        Ok(PdfOperatorVariant::InlineImage(Self::new(
            Dictionary::new(BTreeMap::new()),
            Vec::new(),
        )))
    }

    fn call<T: PdfOperatorBackend>(&self, _backend: &mut T) -> Result<(), BackendError<T>> {
        Ok(())
    }
}
