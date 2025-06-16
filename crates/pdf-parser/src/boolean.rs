use pdf_tokenizer::{PdfToken, error::TokenizerError};
use thiserror::Error;

use crate::{PdfParser, traits::BooleanParser};

#[derive(Debug, PartialEq, Error)]
pub enum BooleanError {
    #[error("Invalid token for boolean object, expected 't' or 'f'")]
    InvalidToken,
    #[error("Failed to parse boolean keyword: {err}")]
    FailedToParseKeyword { err: String },
    #[error("Tokenizer error: {0}")]
    TokenizerError(#[from] TokenizerError),
}

impl BooleanParser for PdfParser<'_> {
    type ErrorType = BooleanError;

    /// Parses a PDF boolean object from the current position in the input stream.
    ///
    /// According to PDF 1.7 Specification (Section 7.3.2), a boolean object:
    ///
    /// # Format
    ///
    /// - Is represented by one of two literal keywords: `true` or `false`.
    /// - These keywords are case-sensitive.
    ///
    /// # Example Inputs
    ///
    /// ```text
    /// true
    /// false
    /// ```
    fn parse_boolean(&mut self) -> Result<bool, Self::ErrorType> {
        const BOOLEAN_LITERAL_TRUE: &[u8] = b"true";
        const BOOLEAN_LITERAL_FALSE: &[u8] = b"false";

        let expected_literal = match self.tokenizer.peek() {
            Some(PdfToken::Alphabetic(b't')) => BOOLEAN_LITERAL_TRUE,
            Some(PdfToken::Alphabetic(b'f')) => BOOLEAN_LITERAL_FALSE,
            _ => return Err(BooleanError::InvalidToken),
        };

        self.read_keyword(expected_literal).map_err(|source| {
            BooleanError::FailedToParseKeyword {
                err: source.to_string(),
            }
        })?;

        Ok(expected_literal == BOOLEAN_LITERAL_TRUE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_boolean_valid() {
        let valid_inputs: Vec<(&[u8], bool)> = vec![
            (b"true ", true),
            (b"false ", false),
            (b"true\n", true),
            (b"false\t", false),
        ];

        for (input, expected) in valid_inputs {
            let mut parser = PdfParser::from(input);
            let value = parser.parse_boolean().unwrap();
            assert_eq!(value, expected);
        }
    }

    #[test]
    fn test_parse_boolean_invalid() {
        let invalid_inputs: Vec<&[u8]> = vec![b"tru ", b"fals ", b"truefalse", b"false123"];

        for input in invalid_inputs {
            let mut parser = PdfParser::from(input);
            let result = parser.parse_boolean();
            assert!(
                result.is_err(),
                "Expected error for invalid input `{}`",
                String::from_utf8_lossy(input)
            );
        }
    }
}
