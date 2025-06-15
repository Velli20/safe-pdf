use std::collections::BTreeMap;

use pdf_object::dictionary::Dictionary;
use pdf_tokenizer::{PdfToken, error::TokenizerError};
use thiserror::Error;

use crate::{
    PdfParser,
    traits::{DictionaryParser, NameParser},
};

/// Represents an error that can occur while parsing an array object.
#[derive(Debug, PartialEq, Error)]
pub enum DictionaryError {
    /// Indicates that there was an error while parsing a key within dictionary object.
    #[error("Error while parsing dictionary key: '{0}'")]
    InvalidKey(String),
    /// Indicates that there was an error while parsing a value within dictionary object.
    #[error("Error while parsing dictionary value: {0}")]
    InvalidValue(String),
    #[error("Tokenizer error: {0}")]
    TokenizerError(#[from] TokenizerError),
}

impl DictionaryParser for PdfParser<'_> {
    type ErrorType = DictionaryError;

    /// Parses a PDF dictionary object from the current position in the input stream.
    ///
    /// According to the PDF 1.7 Specification (Section 7.3.7 "Dictionary Objects"):
    /// A dictionary object is an associative table containing pairs of objects, known as
    /// the dictionary's entries.
    ///
    /// # Format
    ///
    /// - Must begin with double left angle brackets (`<<`) and end with double right
    ///   angle brackets (`>>`).
    /// - Contains zero or more key-value pairs.
    /// - Each key must be a PDF Name object (e.g., `/Type`, `/Size`).
    /// - Each value can be any valid PDF object (e.g., another dictionary, an array,
    ///   a number, a string, a name, a boolean, null, or an indirect reference).
    /// - Whitespace is generally ignored between tokens within the dictionary.
    ///
    /// # Example Inputs
    ///
    /// ```text
    /// << >>
    /// << /Type /Catalog /Pages 2 0 R >>
    /// << /Key1 (Value1) /Key2 123 /Key3 [1 2 3] >>
    /// << /NestedDict << /InnerKey /InnerValue >> >>
    /// ```
    ///
    /// # Returns
    ///
    /// A `Dictionary` object containing the parsed key-value pairs,
    /// or a `ParserError` if the input is malformed (e.g., missing delimiters,
    /// invalid key or value syntax, or an unexpected token).
    fn parse_dictionary(&mut self) -> Result<Dictionary, Self::ErrorType> {
        // Expect the opening `<<` of the dictionary.
        self.tokenizer.expect(PdfToken::DoubleLeftAngleBracket)?;

        // Skip whitespace
        self.skip_whitespace();

        let mut dictionary = BTreeMap::new();

        while let Some(token) = self.tokenizer.peek() {
            if let PdfToken::DoubleRightAngleBracket = token {
                break;
            }

            // Skip whitespace
            self.skip_whitespace();

            // Parse key.
            let key = self
                .parse_name()
                .map_err(|e| DictionaryError::InvalidKey(e.to_string()))?;

            self.skip_whitespace();

            // Parse value.
            let value = self
                .parse_object()
                .map_err(|e| DictionaryError::InvalidValue(e.to_string()))?;

            dictionary.insert(key.0, Box::new(value));
            self.skip_whitespace();
        }

        // Consume the closing `>>` of the dictionary.
        self.tokenizer.expect(PdfToken::DoubleRightAngleBracket)?;

        Ok(Dictionary::new(dictionary))
    }
}

#[cfg(test)]
mod tests {

    use crate::{PdfParser, traits::DictionaryParser};

    #[test]
    fn test_dictionary_valid() {
        let inputs:  Vec<(&[u8], usize)> = vec![
            (b"<< >>", 0),
            (b"<< /Type /Catalog >>", 1),
            (b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>", 3),
            (b"<< /Type /Annot /Rect [100 100 200 200] /A << /S /URI /URI (https://example.com) >> >>", 3),
            (b"<< /Author (John Doe) /IsDraft true >>", 2),
            (b"<< /Count 42 /ID <4FAE23> >>", 2),
        ];

        for (input, expected_count) in inputs {
            let mut parser = PdfParser::from(input);
            let result = parser.parse_dictionary().unwrap();

            assert_eq!(
                result.dictionary.len(),
                expected_count,
                "Expected {} elements for input '{}', got {}",
                expected_count,
                String::from_utf8_lossy(input),
                result.dictionary.len()
            );
        }
    }

    #[test]
    fn test_dictionary_invalid() {
        let inputs: Vec<&[u8]> = vec![
            // Missing closing >>
            b"<< /Type /Page",
            // No leading <<
            b"/Type /Page >>",
            // Invalid key format
            b"<< Type /Page >>",
            // Non-name as key
            b"<< (Title) /Something >>",
            // Unterminated string
            b"<< /Title (Missing end >>",
            // Unexpected object inside dictionary
            b"<< /Stream stream... endstream >>",
            // Invalid hex string
            b"<< /ID <Z23G> >>",
        ];

        for input in inputs {
            let mut parser = PdfParser::from(input);
            let result = parser.parse_dictionary();

            assert_eq!(
                result.is_err(),
                true,
                "Expected Err for input '{}', got {:?}",
                String::from_utf8_lossy(input),
                result
            );
        }
    }
}
