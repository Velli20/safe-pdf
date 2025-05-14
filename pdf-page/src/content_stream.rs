use pdf_operator::pdf_operator::PdfOperatorVariant;

use crate::error::PageError;

pub struct ContentStream {
    pub operations: Vec<PdfOperatorVariant>,
}

impl ContentStream {
    pub fn from(input: &[u8]) -> Result<Self, PageError> {
        let operations = PdfOperatorVariant::from(input)?;
        Ok(Self { operations })
    }
}
