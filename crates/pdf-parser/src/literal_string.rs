use pdf_tokenizer::{PdfToken, error::TokenizerError};
use thiserror::Error;

use crate::parser::PdfParser;

/// Represents an error that can occur while parsing a literal string object.
#[derive(Debug, PartialEq, Error)]
pub enum LiteralStringObjectError {
    #[error("Unbalanced parentheses in literal string")]
    UnbalancedParentheses,
    #[error("Tokenizer error: {0}")]
    TokenizerError(#[from] TokenizerError),
    #[error("Too many opening parentheses in literal string")]
    TooManyOpeningParentheses,
}

impl PdfParser<'_> {
    /// Parses a PDF literal string object from the current position in the input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if parentheses are unbalanced or delimiters are missing.
    pub fn parse_literal_string(&mut self) -> Result<Vec<u8>, LiteralStringObjectError> {
        // Expect the opening parenthesis `(`.
        self.tokenizer.expect(PdfToken::LeftParenthesis)?;

        let mut characters = Vec::new();
        let mut depth = 0_usize;
        let mut escaped = false;

        // Read bytes until we find the matching, unescaped closing ')'.
        // We handle the following minimal behaviors:
        // - Balanced parentheses using a depth counter for nested parens
        // - A backslash '\\' escapes the very next character. For '\\', '\(', and '\)',
        //   we emit a single '\\', '(', or ')' respectively (i.e., we do NOT keep the
        //   backslash in the output). For other characters following a backslash, we
        //   emit the character as-is (backslash is ignored), which aligns with the PDF
        //   spec's permissive behavior for unknown escapes.
        // - We still do not normalize line endings present literally in the input; they
        //   are preserved as-is.
        loop {
            // Read exactly one byte; reaching EOF without closing means unbalanced parentheses
            let b = self
                .tokenizer
                .read_exactly(1)?
                .first()
                .copied()
                .ok_or(LiteralStringObjectError::UnbalancedParentheses)?;

            match (escaped, b) {
                // Previous char was a backslash: take this byte literally and clear escape state
                (true, byte) => {
                    // Interpret common escapes and special pairs. For any other byte, we
                    // preserve the backslash and the byte literally (treat as not an escape).
                    match byte {
                        // Common PDF escapes.
                        b'n' => characters.push(b'\n'),
                        b'r' => characters.push(b'\r'),
                        b't' => characters.push(b'\t'),
                        b'b' => characters.push(0x08), // backspace
                        b'f' => characters.push(0x0C), // form feed
                        // Octal escape: up to three octal digits (0-7)
                        b'0'..=b'7' => {
                            // We've already consumed one octal digit in `byte`.
                            let mut value = u32::from(byte.saturating_sub(b'0'));

                            // Look ahead without consuming: take up to two additional octal digits.
                            let lookahead = self.tokenizer.data();
                            let mut extra = 0usize;
                            for &nb in lookahead.iter().take(2) {
                                if matches!(nb, b'0'..=b'7') {
                                    extra = extra.saturating_add(1);
                                } else {
                                    break;
                                }
                            }
                            if extra > 0 {
                                // Consume exactly `extra` digits now.
                                let next_bytes = self.tokenizer.read_exactly(extra)?;
                                for &nb in next_bytes {
                                    value = value.saturating_mul(8)
                                        | u32::from(nb.saturating_sub(b'0'));
                                }
                            }
                            // Clamp to single byte and push.
                            let out = u8::try_from(value & 0xFF).unwrap_or(0);
                            characters.push(out);
                        }
                        // Escaped delimiter/backslash
                        b'(' | b')' | b'\\' => characters.push(byte),
                        // Unknown escape: keep the backslash and the byte
                        other => {
                            characters.push(b'\\');
                            characters.push(other);
                        }
                    }
                    escaped = false;
                }
                // Start escape sequence; do NOT keep the backslash in output
                (false, b'\\') => {
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
                    return Ok(characters);
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
        let valid_inputs: Vec<(&[u8], &[u8])> = vec![
            (b"(Hello, World!)", b"Hello, World!"),
            (b"(This is a test)", b"This is a test"),
            (b"(Nested (parentheses))", b"Nested (parentheses)"),
            (b"(Special characters *!%)", b"Special characters *!%"),
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
        let cases: Vec<(&[u8], &[u8])> = vec![
            (b"(Hello World)", b"Hello World"),
            (b"(Line\nBreak)", b"Line\nBreak"),
            (b"(Carriage\rReturn)", b"Carriage\rReturn"),
            (b"(Tab\tSeparated)", b"Tab\tSeparated"),
            (b"(Back\\Slash)", b"Back\\Slash"),
            (b"(Paren\\(\\)Test)", b"Paren()Test"),
        ];

        for (input, expected) in cases {
            let mut parser = PdfParser::from(input);
            let result = parser.parse_literal_string().unwrap();
            assert_eq!(result, expected, "input: {:?}", input);
        }

        // Octal escapes: (\000\035) => bytes [0x00, 0x1D]
        let input: &[u8] = b"(\\000\\035)";
        let mut parser = PdfParser::from(input);
        let result = parser.parse_literal_string().unwrap();
        assert_eq!(result, vec![0x00, 0x1D]);
    }
}
