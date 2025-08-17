use pdf_tokenizer::{PdfToken, error::TokenizerError};
use thiserror::Error;

use crate::{PdfParser, traits::LiteralStringParser};

/// Represents an error that can occur while parsing a literal string object.
#[derive(Debug, PartialEq, Error)]
pub enum LiteralStringObjectError {
    /// Indicates that the escape sequence is invalid.
    #[error("Invalid escape sequence in literal string")]
    InvalidEscapeSequence,
    /// Indicates that the parentheses are unbalanced.
    #[error("Unbalanced parentheses in literal string")]
    UnbalancedParentheses,
    #[error("Tokenizer error: {0}")]
    TokenizerError(#[from] TokenizerError),
}

impl LiteralStringParser for PdfParser<'_> {
    type ErrorType = LiteralStringObjectError;

    /// Parses a PDF literal string object from the current position in the input stream.
    ///
    /// According to the PDF 1.7 Specification (Section 7.3.4.2), a literal string:
    ///
    /// # Format
    ///
    /// - Must begin with a left parenthesis `(` and end with a right parenthesis `)`.
    /// - Can contain any characters.
    /// - Parentheses `()` within the string must be balanced (e.g., `(string with (nested) parens)`).
    ///   The parser correctly handles nested parentheses by maintaining a depth count.
    ///
    /// # Note on Escape Sequences and Line Endings
    ///
    /// The PDF specification (Section 7.3.4.2) defines escape sequences (e.g., `\n` for newline,
    /// `\\` for backslash, `\ddd` for octal codes). It also states that line endings
    /// (CR, LF, or CRLF) within a literal string should be treated as a single line feed (LF)
    /// character.
    ///
    /// This current parser implementation reads characters literally:
    /// - It does **not** process standard PDF escape sequences. For example, a PDF string `(line1\nline2)`
    ///   would be parsed into a Rust string containing the literal characters `\` and `n`.
    /// - It does **not** normalize line endings. A PDF string `(line1\r\nline2)` would retain
    ///   the `\r\n` sequence in the resulting Rust string.
    ///
    /// # Example Inputs
    ///
    /// ```text
    /// (This is a string)
    /// (Strings may contain newlines
    /// and such.)
    /// (Strings may contain balanced parentheses (such as these).)
    /// (This string contains \n and \\ literally, not as escapes)
    /// ```
    ///
    /// # Returns
    ///
    /// A `LiteralString` object containing the characters between the outermost parentheses,
    /// or a `ParserError` if the parentheses are unbalanced, delimiters are missing, or an
    /// unexpected token is encountered.
    fn parse_literal_string(&mut self) -> Result<String, Self::ErrorType> {
        // Expect the opening parenthesis `(`.
        self.tokenizer.expect(PdfToken::LeftParenthesis)?;

        let mut characthers = Vec::new();
        let mut depth = 0_usize;

        // Read the content of the literal string until the closing parenthesis `)`.
        loop {
            let content = self.tokenizer.read_while_u8(|b| b != b')' && b != b'(');
            if !content.is_empty() {
                characthers.extend_from_slice(content);
            }
            if let Some(token) = self.tokenizer.read() {
                match token {
                    PdfToken::LeftParenthesis => {
                        // Nested parenthesis, increment depth.
                        depth += 1;
                        characthers.push(b'(');
                        continue;
                    }
                    PdfToken::RightParenthesis => {
                        if depth == 0 {
                            // End of a literal string

                            return Ok(String::from_utf8_lossy(&characthers).to_string());
                        } else {
                            // Nested parenthesis
                            depth -= 1;
                            characthers.push(b')');
                            continue;
                        }
                    }
                    _ => {
                        // Invalid token for a literal string
                        return Err(LiteralStringObjectError::UnbalancedParentheses);
                    }
                }
            }
            break;
        }

        // If we reach here, it means we have an unbalanced parenthesis.
        Err(LiteralStringObjectError::UnbalancedParentheses)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_literal_string_valid() {
        let valid_inputs: Vec<(&[u8], &str)> = vec![
            (b"(Hello, World!)", "Hello, World!"),
            (b"(This is a test)", "This is a test"),
            (b"(Nested (parentheses))", "Nested (parentheses)"),
            (b"(Special characters *!%)", "Special characters *!%"),
        ];

        for (input, expected) in valid_inputs {
            let mut parser = PdfParser::from(input);

            let result = parser.parse_literal_string().unwrap();
            assert_eq!(result, expected);
        }
    }

    #[test]
    fn test_parse_literal_string_invalid() {
        let invalid_inputs: Vec<&[u8]> = vec![
            b"(Unbalanced parentheses", // Missing closing parenthesis
            b"Unbalanced parentheses)", // Missing opening parenthesis
                                        //b"(Invalid \\ escape)",     // Invalid escape sequence
        ];

        for input in invalid_inputs {
            let mut parser = PdfParser::from(input);
            let result = parser.parse_literal_string();
            assert!(
                result.is_err(),
                "Expected error for invalid input `{}`",
                String::from_utf8_lossy(input)
            );
        }
    }
}
