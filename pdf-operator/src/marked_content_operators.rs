use crate::{
    error::PdfPainterError,
    pdf_operator::{Operands, PdfOperatorVariant},
};

/// Begins a marked-content sequence. (PDF operator `BMC`)
/// Marked-content sequences associate a tag with a sequence of content stream operations.
#[derive(Debug, Clone, PartialEq)]
pub struct BeginMarkedContent {
    /// The tag indicating the role or nature of the marked-content sequence.
    tag: String,
}

impl BeginMarkedContent {
    pub const fn operator_name() -> &'static str {
        "BMC"
    }

    pub fn new(tag: String) -> Self {
        Self { tag }
    }

    pub fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfPainterError> {
        let tag = operands.get_str()?;
        Ok(PdfOperatorVariant::BeginMarkedContent(Self::new(tag)))
    }
}

/// Begins a marked-content sequence with an associated property list. (PDF operator `BDC`)
/// The property list can be either a name (referring to an entry in the Properties subdictionary
/// of the current resource dictionary) or an inline dictionary.
#[derive(Debug, Clone, PartialEq)]
pub struct BeginMarkedContentWithProps {
    /// The tag indicating the role or nature of the marked-content sequence.
    tag: String,
    /// The property list, which can be a name (of an entry in the resource dictionary's Properties subdictionary) or an inline dictionary.
    properties: String,
}

impl BeginMarkedContentWithProps {
    pub const fn operator_name() -> &'static str {
        "BDC"
    }

    pub fn new(tag: String, properties: String) -> Self {
        Self { tag, properties }
    }

    pub fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfPainterError> {
        let tag = operands.get_str()?;
        let properties = operands.get_str()?;
        Ok(PdfOperatorVariant::BeginMarkedContentWithProps(Self::new(
            tag, properties,
        )))
    }
}

/// Ends a marked-content sequence begun by a `BMC` or `BDC` operator. (PDF operator `EMC`)
#[derive(Debug, Clone, PartialEq)]
pub struct EndMarkedContent;

impl EndMarkedContent {
    pub const fn operator_name() -> &'static str {
        "EMC"
    }

    pub const fn new() -> Self {
        Self
    }

    pub fn read(_operands: &mut Operands) -> Result<PdfOperatorVariant, PdfPainterError> {
        Ok(PdfOperatorVariant::EndMarkedContent(Self::new()))
    }
}
