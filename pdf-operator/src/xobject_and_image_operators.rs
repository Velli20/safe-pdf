use crate::{error::PdfPainterError, pdf_operator::PdfOperatorVariant};

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

    pub fn read() -> Result<PdfOperatorVariant, PdfPainterError> {
        Err(PdfPainterError::UnimplementedOperation(
            Self::operator_name(),
        ))
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

    pub fn new() -> Self {
        Self
    }

    pub fn read() -> Result<PdfOperatorVariant, PdfPainterError> {
        Err(PdfPainterError::UnimplementedOperation(
            Self::operator_name(),
        ))
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

    pub fn read() -> Result<PdfOperatorVariant, PdfPainterError> {
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

    pub fn new() -> Self {
        Self
    }

    pub fn read() -> Result<PdfOperatorVariant, PdfPainterError> {
        Err(PdfPainterError::UnimplementedOperation(
            Self::operator_name(),
        ))
    }
}
