use pdf_tokenizer::{PdfToken, error::TokenizerError};
use thiserror::Error;

use crate::{parser::PdfParser, traits::LiteralStringParser};

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
    #[error("Too many opening parentheses in literal string")]
    TooManyOpeningParentheses,
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

        let mut characters = Vec::new();
        let mut depth = 0_usize;
        let mut escaped = false;

        // Read bytes until we find the matching, unescaped closing ')'.
        // We handle the following minimal behaviors:
        // - Balanced parentheses using a depth counter for nested parens
        // - A backslash '\\' escapes the very next character, including '(' and ')'
        //   so that an escaped ')' does not terminate the string
        // We intentionally do not interpret escape sequences (e.g. \n) beyond
        // treating the backslash as an escape for the next byte; content is kept literal.
        loop {
            // Read exactly one byte; reaching EOF without closing means unbalanced parentheses
            let b = match self.tokenizer.read_excactly(1) {
                Ok(bytes) if !bytes.is_empty() => bytes[0],
                _ => return Err(LiteralStringObjectError::UnbalancedParentheses),
            };

            match (escaped, b) {
                // Previous char was a backslash: take this byte literally and clear escape state
                (true, byte) => {
                    characters.push(byte);
                    escaped = false;
                }
                // Start escape sequence; we keep the backslash literally
                (false, b'\\') => {
                    characters.push(b'\\');
                    escaped = true;
                }
                // Nested opening parenthesis
                (false, b'(') => {
                    depth = depth
                        .checked_add(1)
                        .ok_or(LiteralStringObjectError::TooManyOpeningParentheses)?;
                    characters.push(b'(');
                }
                // Possible closing of the literal or a nested close
                (false, b')') if depth == 0 => {
                    return Ok(String::from_utf8_lossy(&characters).to_string());
                }
                (false, b')') => {
                    depth = depth
                        .checked_sub(1)
                        .ok_or(LiteralStringObjectError::UnbalancedParentheses)?;
                    characters.push(b')');
                }
                // Regular byte
                (false, byte) => characters.push(byte),
            }
        }
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

    #[test]
    fn test_parse_literal_string_with_escapes() {
        let cases: Vec<(&[u8], &str)> = vec![
            // Escaped right parenthesis should be taken literally and not terminate the string
            (b"(\\))", "\\)"),
            // Escaped left parenthesis should be taken literally
            (b"(\\())", "\\("),
            // Escaped parentheses inside text remain literal; no nesting occurs due to escapes
            (b"(foo \\(bar\\) baz)", "foo \\(bar\\) baz"),
            // Mix of real nested parens and an escaped right paren inside
            (
                b"(outer (inner \\) still inner) end)",
                "outer (inner \\) still inner) end",
            ),
            // Escaped backslash results in a literal backslash character in output
            (b"(\\\\)", "\\\\"),
            // Escape sequence like \n is kept as backslash + 'n', not a newline
            (b"(\\n)", "\\n"),
            // Escaped parens around content
            (b"(\\(nested\\))", "\\(nested\\)"),
        ];

        for (input, expected) in cases {
            let mut parser = PdfParser::from(input);
            let result = parser.parse_literal_string().unwrap();
            assert_eq!(result, expected, "input: {:?}", input);
        }
    }
}
