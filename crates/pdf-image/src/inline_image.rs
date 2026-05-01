use pdf_object::dictionary::Dictionary;

/// Canonical parsed representation of a PDF inline image.
#[derive(Debug, Clone, PartialEq)]
pub struct InlineImage {
    dictionary: Dictionary,
    data: Vec<u8>,
}

impl InlineImage {
    /// Creates a new inline image from its parsed dictionary and raw payload bytes.
    pub fn new(dictionary: Dictionary, data: Vec<u8>) -> Self {
        Self { dictionary, data }
    }

    /// Returns the parsed inline-image dictionary.
    pub fn dictionary(&self) -> &Dictionary {
        &self.dictionary
    }

    /// Returns the raw inline-image payload bytes.
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Splits the inline image into its parsed dictionary and raw payload.
    pub fn into_parts(self) -> (Dictionary, Vec<u8>) {
        (self.dictionary, self.data)
    }
}
