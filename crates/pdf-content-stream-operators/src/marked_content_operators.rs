use std::sync::Arc;

use crate::{
    error::PdfOperatorError,
    operands::Operands,
    operator_trait::PdfOperator,
    pdf_operator_backend::{BackendError, PdfOperatorBackend},
    variants::PdfOperatorVariant,
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
}

impl PdfOperator for BeginMarkedContent {
    const NAME: &'static [u8] = b"BMC";

    const OPERAND_COUNT: Option<usize> = Some(1);

    fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        let tag = operands.get_string_bytes()?;
        Ok(PdfOperatorVariant::BeginMarkedContent(Self::new(tag)))
    }

    fn call<T: PdfOperatorBackend>(&self, backend: &mut T) -> Result<(), BackendError<T>> {
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
}

impl BeginMarkedContentWithProps {
    pub fn new(tag: Arc<[u8]>) -> Self {
        Self { tag }
    }
}

impl PdfOperator for BeginMarkedContentWithProps {
    const NAME: &'static [u8] = b"BDC";

    const OPERAND_COUNT: Option<usize> = Some(2);

    fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        let tag = operands.get_string_bytes()?;
        Ok(PdfOperatorVariant::BeginMarkedContentWithProps(Self::new(
            tag,
        )))
    }

    fn call<T: PdfOperatorBackend>(&self, backend: &mut T) -> Result<(), BackendError<T>> {
        backend.begin_marked_content_with_properties(&self.tag)
    }
}

/// Ends a marked-content sequence begun by a `BMC` or `BDC` operator.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EndMarkedContent;

impl PdfOperator for EndMarkedContent {
    const NAME: &'static [u8] = b"EMC";

    const OPERAND_COUNT: Option<usize> = Some(0);

    fn read(_operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        Ok(PdfOperatorVariant::EndMarkedContent(Self))
    }

    fn call<T: PdfOperatorBackend>(&self, backend: &mut T) -> Result<(), BackendError<T>> {
        backend.end_marked_content()
    }
}
