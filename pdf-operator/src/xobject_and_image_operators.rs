use crate::{
    error::PdfPainterError,
    pdf_operator::{Operands, PdfOperatorVariant},
};

/// Invokes a named XObject. (PDF operator `Do`)
/// XObjects are external objects such as images or self-contained page descriptions (Form XObjects).
#[derive(Debug, Clone, PartialEq)]
pub struct InvokeXObject {
    /// The name of the XObject resource to invoke, as defined in the resource dictionary.
    name: String,
}

impl InvokeXObject {
    pub const fn operator_name() -> &'static str {
        "Do"
    }

    pub fn new(name: String) -> Self {
        Self { name }
    }

    pub fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfPainterError> {
        let name = operands.get_name()?;
        Ok(PdfOperatorVariant::InvokeXObject(Self::new(name)))
    }
}

/// Begins an inline image object. (PDF operator `BI`)
/// This operator is followed by key-value pairs defining the image's properties, then the `ID` operator and image data, and finally `EI`.
#[derive(Debug, Clone, PartialEq)]
pub struct BeginInlineImage;

impl BeginInlineImage {
    pub const fn operator_name() -> &'static str {
        "BI"
    }

    pub const fn new() -> Self {
        Self
    }

    pub fn read(_operands: &mut Operands) -> Result<PdfOperatorVariant, PdfPainterError> {
        // The BI operator itself does not consume operands from the stack.
        // The inline image dictionary key-value pairs follow BI directly in the stream.
        // A full parser would need to enter a special state here to parse those pairs,
        // then the ID operator, then image data, then EI.
        // This function merely constructs the BeginInlineImage marker.
        Ok(PdfOperatorVariant::BeginInlineImage(Self::new()))
    }
}

/// Represents the image data within an inline image object. (PDF operator `ID`)
/// The `ID` operator itself marks the beginning of the image data stream, which is then followed by the actual image data.
/// This struct holds that image data.
#[derive(Debug, Clone, PartialEq)]
pub struct InlineImageData {
    /// The raw byte data of the inline image.
    data: Vec<u8>,
}

impl InlineImageData {
    pub const fn operator_name() -> &'static str {
        "ID"
    }

    pub fn new(data: Vec<u8>) -> Self {
        Self { data }
    }

    pub fn read(_operands: &mut Operands) -> Result<PdfOperatorVariant, PdfPainterError> {
        // The ID (Image Data) operator itself does not have preceding operands that form the image data.
        // The image data stream follows the ID token and is terminated by EI.
        // The `_operands` received here would typically contain the key-value pairs of the
        // inline image dictionary if the main parser collected them as generic operands before ID.
        // This `read` function, within the current `Operands` model, cannot access or parse
        // the actual image data that follows the ID token.
        // Proper parsing of inline image data requires special handling in the main parser loop.
        Err(PdfPainterError::UnimplementedOperation(
            Self::operator_name(),
        ))
    }
}

/// Ends an inline image object. (PDF operator `EI`)
#[derive(Debug, Clone, PartialEq)]
pub struct EndInlineImage;

impl EndInlineImage {
    pub const fn operator_name() -> &'static str {
        "EI"
    }

    pub const fn new() -> Self {
        Self
    }

    pub fn read(_operands: &mut Operands) -> Result<PdfOperatorVariant, PdfPainterError> {
        // The EI operator does not take any operands from the stack.
        // It simply marks the end of the inline image data.
        Ok(PdfOperatorVariant::EndInlineImage(Self::new()))
    }
}
