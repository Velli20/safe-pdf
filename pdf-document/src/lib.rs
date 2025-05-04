use pdf_object::{Value, version::Version};
use pdf_parser::{ParseObject, PdfParser};

/// Represents a PDF document.
pub struct PdfDocument {
    /// The version of the PDF document.
    pub version: Version,
    /// The objects in the PDF document.
    pub objects: Vec<Value>,
}

impl PdfDocument {
    pub fn from(input: &[u8]) -> Self {
        let mut parser = PdfParser::from(input);
        let version: Version = parser.parse().unwrap();

        let mut objects = Vec::new();
        loop {
            let object = parser.parse_object().unwrap();
            println!("{:?}", object);
            match object {
                Value::EndOfFile => break,
                _ => {}
            }

            objects.push(object);
        }
        PdfDocument { version, objects }
    }
}
