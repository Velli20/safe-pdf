use crate::PdfOperator;

/// Invokes a named XObject. (PDF operator `Do`)
/// XObjects are external objects such as images or self-contained page descriptions (Form XObjects).
#[derive(Debug, Clone, PartialEq)]
pub struct InvokeXObject {
    /// The name of the XObject resource to invoke, as defined in the resource dictionary.
    name: String,
}

impl PdfOperator for InvokeXObject {
    fn operator() -> &'static str {
        "Do"
    }
}

impl InvokeXObject {
    pub fn new(name: String) -> Self {
        Self { name }
    }
}

/// Begins an inline image object. (PDF operator `BI`)
/// This operator is followed by key-value pairs defining the image's properties, then the `ID` operator and image data, and finally `EI`.
#[derive(Debug, Clone, PartialEq)]
pub struct BeginInlineImage;
impl PdfOperator for BeginInlineImage {
    fn operator() -> &'static str {
        "BI"
    }
}

impl BeginInlineImage {
    pub fn new() -> Self {
        Self
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

impl PdfOperator for InlineImageData {
    fn operator() -> &'static str {
        "ID"
    }
}

impl InlineImageData {
    pub fn new(data: Vec<u8>) -> Self {
        Self { data }
    }
}

/// Ends an inline image object. (PDF operator `EI`)
#[derive(Debug, Clone, PartialEq)]
pub struct EndInlineImage;
impl PdfOperator for EndInlineImage {
    fn operator() -> &'static str {
        "EI"
    }
}

impl EndInlineImage {
    pub fn new() -> Self {
        Self
    }
}
