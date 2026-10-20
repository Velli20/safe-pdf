use std::sync::Arc;

use pdf_object_reader::{ReadResult, object_variant::ObjectVariant, pdf_string::PdfString};

use crate::{
    OperandContext,
    pdf_operator_backend::{BackendError, PdfOperatorBackend},
};

/// Begins a marked-content sequence.
/// Marked-content sequences associate a tag with a sequence of content stream operations.
#[derive(Debug, Clone, PartialEq)]
pub struct BeginMarkedContent {
    /// The tag indicating the role or nature of the marked-content sequence.
    tag: Arc<[u8]>,
}

impl BeginMarkedContent {
    pub fn new(tag: Arc<[u8]>) -> Self {
        Self { tag }
    }

    /// Reads the operator from its operands.
    pub(crate) fn read(operands: &mut OperandContext<'_, '_>) -> ReadResult<Self> {
        let tag = operands.at::<PdfString>(0)?.bytes;
        Ok(Self::new(tag))
    }

    /// Dispatches the operator to `backend`.
    pub(crate) fn call<T: PdfOperatorBackend>(
        &self,
        backend: &mut T,
    ) -> Result<(), BackendError<T>> {
        backend.begin_marked_content(&self.tag)
    }
}

/// Begins a marked-content sequence with an associated property list.
/// The property list can be either a name (referring to an entry in the Properties subdictionary
/// of the current resource dictionary) or an inline dictionary.
#[derive(Debug, Clone, PartialEq)]
pub struct BeginMarkedContentWithProps {
    /// The tag indicating the role or nature of the marked-content sequence.
    tag: Arc<[u8]>,
    /// The name of a property list in the `/Properties` resource subdictionary, or
    /// `None` when the property list is an inline dictionary.
    properties: Option<Arc<[u8]>>,
}

impl BeginMarkedContentWithProps {
    pub fn new(tag: Arc<[u8]>, properties: Option<Arc<[u8]>>) -> Self {
        Self { tag, properties }
    }

    /// Reads the operator from its operands.
    pub(crate) fn read(operands: &mut OperandContext<'_, '_>) -> ReadResult<Self> {
        let tag = operands.at::<PdfString>(0)?.bytes;
        let properties = match operands.array().get(1) {
            Some(ObjectVariant::String(_)) => Some(operands.at::<PdfString>(1)?.bytes),
            _ => None,
        };
        Ok(Self::new(tag, properties))
    }

    /// Dispatches the operator to `backend`.
    pub(crate) fn call<T: PdfOperatorBackend>(
        &self,
        backend: &mut T,
    ) -> Result<(), BackendError<T>> {
        backend.begin_marked_content_with_properties(&self.tag, self.properties.as_deref())
    }
}
