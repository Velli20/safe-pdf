use crate::{
    error::PdfOperatorError,
    pdf_operator::{Operands, PdfOperator, PdfOperatorVariant},
};

/// Begins a marked-content sequence.
/// Marked-content sequences associate a tag with a sequence of content stream operations.
#[derive(Debug, Clone, PartialEq)]
pub struct BeginMarkedContent {
    /// The tag indicating the role or nature of the marked-content sequence.
    tag: String,
}

impl BeginMarkedContent {
    pub fn new(tag: String) -> Self {
        Self { tag }
    }
}

impl PdfOperator for BeginMarkedContent {
    const NAME: &'static str = "BMC";

    const OPERAND_COUNT: usize = 1;

    fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        let tag = operands.get_str()?;
        Ok(PdfOperatorVariant::BeginMarkedContent(Self::new(tag)))
    }
}

/// Begins a marked-content sequence with an associated property list.
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
    pub fn new(tag: String, properties: String) -> Self {
        Self { tag, properties }
    }
}

impl PdfOperator for BeginMarkedContentWithProps {
    const NAME: &'static str = "BDC";

    const OPERAND_COUNT: usize = 2;

    fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        let tag = operands.get_str()?;
        let properties = operands.get_str()?;
        Ok(PdfOperatorVariant::BeginMarkedContentWithProps(Self::new(
            tag, properties,
        )))
    }
}

/// Ends a marked-content sequence begun by a `BMC` or `BDC` operator.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EndMarkedContent;

impl PdfOperator for EndMarkedContent {
    const NAME: &'static str = "EMC";

    const OPERAND_COUNT: usize = 0;

    fn read(_operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        Ok(PdfOperatorVariant::EndMarkedContent(Self::default()))
    }
}
