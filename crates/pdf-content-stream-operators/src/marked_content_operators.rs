use pdf_object_reader::{ReadResult, object_variant::ObjectVariant, pdf_name::PdfName};

use crate::OperandContext;

/// Begins a marked-content sequence with an associated property list.
/// The property list can be either a name (referring to an entry in the Properties subdictionary
/// of the current resource dictionary) or an inline dictionary.
#[derive(Debug, Clone, PartialEq)]
pub struct BeginMarkedContentWithProps {
    /// The tag indicating the role or nature of the marked-content sequence.
    tag: PdfName,
    /// The name of a property list in the `/Properties` resource subdictionary, or
    /// `None` when the property list is an inline dictionary.
    properties: Option<PdfName>,
}

impl BeginMarkedContentWithProps {
    pub fn new(tag: PdfName, properties: Option<PdfName>) -> Self {
        Self { tag, properties }
    }

    /// Reads the operator from its operands.
    pub(crate) fn read(operands: &mut OperandContext<'_, '_>) -> ReadResult<Self> {
        let tag = operands.at(0)?;
        let properties = match operands.array().get(1) {
            Some(ObjectVariant::String(_)) => Some(operands.at(1)?),
            _ => None,
        };
        Ok(Self::new(tag, properties))
    }

    /// The tag indicating the role or nature of the marked-content sequence.
    pub fn tag(&self) -> &[u8] {
        self.tag.as_bytes()
    }

    /// The name of the property list in the `/Properties` resource subdictionary, or `None`
    /// when the property list is an inline dictionary.
    pub fn properties(&self) -> Option<&[u8]> {
        self.properties.as_ref().map(PdfName::as_bytes)
    }
}
