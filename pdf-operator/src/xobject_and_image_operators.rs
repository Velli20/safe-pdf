use crate::{
    error::PdfOperatorError,
    pdf_operator::{Operands, PdfOperator, PdfOperatorVariant},
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
    const NAME: &'static str = "Do";

    const OPERAND_COUNT: usize = 1;

    fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        let name = operands.get_name()?;
        Ok(PdfOperatorVariant::InvokeXObject(Self::new(name)))
    }
}

/// Begins an inline image object.
/// This operator is followed by key-value pairs defining the image's properties,
/// then the `ID` operator and image data, and finally `EI`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BeginInlineImage;

impl PdfOperator for BeginInlineImage {
    const NAME: &'static str = "BI";

    const OPERAND_COUNT: usize = 0;

    fn read(_operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        // The BI operator itself does not consume operands from the stack.
        // The inline image dictionary key-value pairs follow BI directly in the stream.
        // A full parser would need to enter a special state here to parse those pairs,
        // then the ID operator, then image data, then EI.
        // This function merely constructs the BeginInlineImage marker.
        Ok(PdfOperatorVariant::BeginInlineImage(Self::default()))
    }
}

/// Represents the image data within an inline image object.
/// The `ID` operator itself marks the beginning of the image data stream, which is then followed by the actual image data.
/// This struct holds that image data.
#[derive(Debug, Clone, PartialEq)]
pub struct InlineImageData {
    /// The raw byte data of the inline image.
    data: Vec<u8>,
}

impl PdfOperator for InlineImageData {
    const NAME: &'static str = "ID";

    const OPERAND_COUNT: usize = 0;

    fn read(_operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        // The ID (Image Data) operator itself does not have preceding operands that form the image data.
        // The image data stream follows the ID token and is terminated by EI.
        // The `_operands` received here would typically contain the key-value pairs of the
        // inline image dictionary if the main parser collected them as generic operands before ID.
        // This `read` function, within the current `Operands` model, cannot access or parse
        // the actual image data that follows the ID token.
        // Proper parsing of inline image data requires special handling in the main parser loop.
        Err(PdfOperatorError::UnimplementedOperation(Self::NAME))
    }
}

/// Ends an inline image object.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EndInlineImage;

impl PdfOperator for EndInlineImage {
    const NAME: &'static str = "EI";

    const OPERAND_COUNT: usize = 0;

    fn read(_operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        // The EI operator does not take any operands from the stack.
        // It simply marks the end of the inline image data.
        Ok(PdfOperatorVariant::EndInlineImage(Self::default()))
    }
}
