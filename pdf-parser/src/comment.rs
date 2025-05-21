use pdf_object::comment::Comment;
use pdf_tokenizer::PdfToken;

use crate::{ParseObject, PdfParser};

impl ParseObject<Comment> for PdfParser<'_> {
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
    fn parse(&mut self) -> Result<Comment, crate::error::ParserError> {
        self.tokenizer.expect(PdfToken::Percent)?;
        // Read until the end of the line.
        let text = self.tokenizer.read_while_u8(|c| c != b'\n' && c != b'\r');
        let text = String::from_utf8_lossy(text).to_string();
        self.read_end_of_line_marker()?;
        Ok(Comment::new(text))
    }
}

#[cfg(test)]
mod tests {
    use crate::error::ParserError;

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
            let result: Comment = parser.parse().unwrap();
            assert_eq!(result.text(), expected);
        }
    }

    #[test]
    fn test_parse_comment_invalid() {
        let invalid_inputs: Vec<&[u8]> = vec![
            b"This is not a comment", // Missing '%' at the start
        ];

        for input in invalid_inputs {
            let mut parser = PdfParser::from(input);
            let result: Result<Comment, ParserError> = parser.parse();
            assert!(
                result.is_err(),
                "Expected error for invalid input `{}`",
                String::from_utf8_lossy(input)
            );
        }
    }
}
