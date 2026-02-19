use pdf_tokenizer::PdfToken;

use crate::{error::ParserError, parser::PdfParser};

impl PdfParser<'_> {
    /// Parses a PDF boolean object from the current position in the input stream.
    ///
    /// # Returns
    ///
    /// Returns a `ParserError` if the input does not match either keyword or if it is not followed by a valid delimiter.
    pub fn parse_boolean(&mut self) -> Result<bool, ParserError> {
        const BOOLEAN_LITERAL_TRUE: &[u8] = b"true";
        const BOOLEAN_LITERAL_FALSE: &[u8] = b"false";

        let expected_literal = match self.tokenizer.peek() {
            Some(PdfToken::Alphabetic(b't')) => BOOLEAN_LITERAL_TRUE,
            Some(PdfToken::Alphabetic(b'f')) => BOOLEAN_LITERAL_FALSE,
            Some(_) => return Err(ParserError::InvalidToken('o')),
            None => return Err(ParserError::UnexpectedEndOfFile),
        };

        self.read_keyword(expected_literal)?;

        Ok(expected_literal == BOOLEAN_LITERAL_TRUE)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
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
