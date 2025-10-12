use pdf_tokenizer::{PdfToken, error::TokenizerError};
use thiserror::Error;

use crate::{parser::PdfParser, traits::CommentParser};

#[derive(Debug, PartialEq, Error)]
pub enum CommentError {
    #[error("Tokenizer error: {0}")]
    TokenizerError(#[from] TokenizerError),
    #[error("Failed to read end-of-line marker after comment: {err}")]
    MissingEOL { err: String },
}

impl CommentParser for PdfParser<'_> {
    type ErrorType = CommentError;

    /// Parses a PDF comment object from the current position in the input stream.
    ///
    /// According to the PDF 1.7 Specification (Section 7.2.3), comments:
    ///
    /// # Format
    ///
    /// - Must begin with a percent sign (`%`).
    /// - Extend to the end of the line (EOL), which can be a carriage return (CR),
    ///   a line feed (LF), or a CR followed by an LF.
    /// - The comment includes all characters after the `%` up to, but not including,
    ///   the EOL marker(s).
    /// - Comments are treated as whitespace by the PDF reader and are typically ignored,
    ///   but they can contain metadata or other information.
    ///
    /// # Example Inputs
    ///
    /// ```text
    /// % This is a comment
    /// %Another comment\r
    /// % Comment with special characters *!%\n
    /// ```
    ///
    /// # Returns
    ///
    /// A `Comment` object containing the text of the comment (excluding the leading `%`
    /// and trailing EOL marker) or an error if the input does not start with `%`.
    fn parse_comment(&mut self) -> Result<String, Self::ErrorType> {
        self.tokenizer.expect(PdfToken::Percent)?;
        // Read until the end of the line.
        let text = self.tokenizer.read_while_u8(|c| c != b'\n' && c != b'\r');
        let text = String::from_utf8_lossy(text).to_string();
        self.read_end_of_line_marker()
            .map_err(|err| CommentError::MissingEOL {
                err: err.to_string(),
            })?;
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
