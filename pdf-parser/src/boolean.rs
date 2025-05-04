use pdf_object::boolean::Boolean;
use pdf_tokenizer::PdfToken;

use crate::{ParseObject, PdfParser, error::ParserError};

impl ParseObject<Boolean> for PdfParser<'_> {
    /// Parses a boolean object from the current position in the input stream.
    ///
    /// According to the PDF 1.7 Specification, Section 7.3.2:
    /// - Boolean objects are represented by the keywords `true` and `false`.
    fn parse(&mut self) -> Result<Boolean, ParserError> {
        const BOOLEAN_LITERAL_TRUE: &[u8] = b"true";
        const BOOLEAN_LITERAL_FALSE: &[u8] = b"false";

        let expected_literal = match self.tokenizer.peek()? {
            Some(PdfToken::Alphabetic(b't')) => BOOLEAN_LITERAL_TRUE,
            Some(PdfToken::Alphabetic(b'f')) => BOOLEAN_LITERAL_FALSE,
            _ => return Err(ParserError::InvalidToken),
        };

        self.read_keyword(expected_literal)?;

        Ok(Boolean::new(expected_literal == BOOLEAN_LITERAL_TRUE))
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
            let result = parser.parse().unwrap();
            let Boolean(value) = result;
            assert_eq!(value, expected);
        }
    }

    #[test]
    fn test_parse_boolean_invalid() {
        let invalid_inputs: Vec<&[u8]> = vec![b"tru ", b"fals ", b"truefalse", b"false123"];

        for input in invalid_inputs {
            let mut parser = PdfParser::from(input);
            let result: Result<Boolean, ParserError> = parser.parse();
            assert!(
                result.is_err(),
                "Expected error for invalid input `{}`",
                String::from_utf8_lossy(input)
            );
        }
    }
}
