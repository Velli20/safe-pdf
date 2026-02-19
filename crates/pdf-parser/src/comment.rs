use pdf_tokenizer::PdfToken;

use crate::{error::ParserError, parser::PdfParser};

impl PdfParser<'_> {
    /// Parses a PDF comment from the current position in the input stream.
    ///
    /// # Returns
    ///
    /// Returns an error if the input does not start with `%`.
    pub fn parse_comment(&mut self) -> Result<String, ParserError> {
        self.tokenizer.expect(PdfToken::Percent)?;
        // Read until the end of the line.
        let text = self.tokenizer.read_while_u8(|c| c != b'\n' && c != b'\r');
        let text = String::from_utf8_lossy(text).to_string();
        self.try_read_end_of_line_marker()?;
        Ok(text)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_comment_valid() {
        let valid_inputs: Vec<(&[u8], &str)> = vec![
            (b"% This is a comment\n", " This is a comment"),
            (b"%Another comment\r", "Another comment"),
            (
                b"%Comment with special characters *!%\n",
                "Comment with special characters *!%",
            ),
        ];

        for (input, expected) in valid_inputs {
            let mut parser = PdfParser::from(input);
            let result = parser.parse_comment().unwrap();
            assert_eq!(result, expected);
        }
    }

    #[test]
    fn test_parse_comment_invalid() {
        let invalid_inputs: Vec<&[u8]> = vec![
            b"This is not a comment", // Missing '%' at the start
        ];

        for input in invalid_inputs {
            let mut parser = PdfParser::from(input);
            let result = parser.parse_comment();
            assert!(
                result.is_err(),
                "Expected error for invalid input `{}`",
                String::from_utf8_lossy(input)
            );
        }
    }
}
