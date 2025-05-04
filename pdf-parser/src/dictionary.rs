use std::collections::BTreeMap;

use pdf_object::{dictionary::Dictionary, name::Name};
use pdf_tokenizer::PdfToken;

use crate::{ParseObject, PdfParser, error::ParserError};

/// Represents an error that can occur while parsing an array object.
#[derive(Debug, PartialEq)]
pub enum DictionaryError {
    /// Indicates that there was an error while parsing a key within dictionary object.
    InvalidKey(String),
    /// Indicates that there was an error while parsing a value within dictionary object.
    InvalidValue(String),
}

impl ParseObject<Dictionary> for PdfParser<'_> {
    /// Parses a PDF dictionary object from the current position in the input stream.
    ///
    /// Dictionaries are key-value aggregates enclosed in double angle brackets (`<< ... >>`).
    /// Keys must be PDF Name objects (e.g., `/KeyName`), and values can be any valid
    /// PDF object type (booleans, numbers, strings, names, arrays, streams, or other
    /// dictionaries).
    ///
    /// This function expects the parser's current position to be at or before the
    /// opening `<<` marker of the dictionary. It consumes the dictionary content,
    /// including the delimiters, advancing the parser state past the closing `>>`.
    /// Whitespace is generally ignored between tokens as per PDF specification rules.
    ///
    /// See PDF 1.7 Specification, Section 7.3.7 for the full definition of a
    /// dictionary object.
    fn parse(&mut self) -> Result<Dictionary, ParserError> {
        // Expect the opening `<<` of the dictionary.
        self.tokenizer.expect(PdfToken::DoubleLeftAngleBracket)?;

        // Skip whitespace
        self.skip_whitespace();

        let mut dictionary = BTreeMap::new();

        while let Some(token) = self.tokenizer.peek()? {
            if let PdfToken::DoubleRightAngleBracket = token {
                break;
            }

            // Skip whitespace
            self.skip_whitespace();

            // Parse key.
            let key: Name = self
                .parse()
                .map_err(|e| ParserError::from(DictionaryError::InvalidKey(e.to_string())))?;

            self.skip_whitespace();

            // Parse value.
            let value = self
                .parse_object()
                .map_err(|e| ParserError::from(DictionaryError::InvalidValue(e.to_string())))?;

            dictionary.insert(key.0, Box::new(value));
            self.skip_whitespace();
        }

        // Consume the closing `>>` of the dictionary.
        self.tokenizer.expect(PdfToken::DoubleRightAngleBracket)?;

        Ok(Dictionary::new(dictionary))
    }
}

impl std::fmt::Display for DictionaryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DictionaryError::InvalidKey(err) => {
                write!(f, "Error while parsing dictionary key: '{}'", err)
            }
            DictionaryError::InvalidValue(err) => {
                write!(f, "Error while parsing dictionary value: {}", err)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use pdf_object::dictionary::Dictionary;

    use crate::{ParseObject, PdfParser, error::ParserError};

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
            let result: Dictionary = parser.parse().unwrap();

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
            let result: Result<Dictionary, ParserError> = parser.parse();

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
