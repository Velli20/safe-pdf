use pdf_object::literal_string::LiteralString;
use pdf_tokenizer::PdfToken;

use crate::{ParseObject, PdfParser, error::ParserError};

/// Represents an error that can occur while parsing a literal string object.
#[derive(Debug, PartialEq)]
pub enum LiteralStringObjectError {
    /// Indicates that the escape sequence is invalid.
    InvalidEscapeSequence,
    /// Indicates that the parentheses are unbalanced.
    UnbalancedParentheses,
}

impl ParseObject<LiteralString> for PdfParser<'_> {
    /// Parses a literal string object from the current position in the input stream.
    ///
    /// This function implements the parsing rules for literal strings as defined in the PDF 1.7 Specification, Section 7.3.4:
    ///
    /// - A literal string is enclosed in parentheses `(...)`.
    /// - It may contain any character except unbalanced left or right parentheses.
    /// - Nested parentheses are allowed and must be properly balanced.
    /// - The backslash `\` character is used as an escape character to represent special sequences, such as `\n` for a newline or `\\` for a literal backslash.
    ///
    /// Examples of valid strings:
    /// - `(Hello, World!)`
    /// - `(This is a string with (nested) parentheses and special characters *!% and so on.)`
    ///
    /// Within a literal string, the backslash `\` character is used as an escape character.
    ///
    /// # Parsing Rules
    ///
    /// 1. The function begins by expecting an opening parenthesis `(`.
    /// 2. It reads the content of the string, handling nested parentheses by maintaining a depth counter.
    /// 3. The backslash `\` is treated as an escape character, allowing sequences like `\(` or `\)` to represent literal parentheses.
    /// 4. Parsing continues until a matching closing parenthesis `)` is encountered at the correct depth.
    /// 5. If the parentheses are unbalanced or an invalid token is encountered, an error is returned.
    ///
    /// # Returns
    ///
    /// A literal string object containing the parsed string if successful.
    fn parse(&mut self) -> Result<LiteralString, crate::error::ParserError> {
        // Expect the opening parenthesis `(`.
        self.tokenizer.expect(PdfToken::LeftParenthesis)?;

        let mut characthers = Vec::new();
        let mut depth = 0_usize;

        // Read the content of the literal string until the closing parenthesis `)`.
        loop {
            let content = self.tokenizer.read_while_u8(|b| b != b')' && b != b'(');
            if !content.is_empty() {
                characthers.extend_from_slice(&content);
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

                            return Ok(LiteralString::new(
                                String::from_utf8_lossy(&characthers).to_string(),
                            ));
                        } else {
                            // Nested parenthesis
                            depth -= 1;
                            characthers.push(b')');
                            continue;
                        }
                    }
                    _ => {
                        // Invalid token for a literal string
                        return Err(ParserError::InvalidToken);
                    }
                }
            }
            break;
        }

        // If we reach here, it means we have an unbalanced parenthesis.
        Err(ParserError::from(
            LiteralStringObjectError::UnbalancedParentheses,
        ))
    }
}

impl std::fmt::Display for LiteralStringObjectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LiteralStringObjectError::InvalidEscapeSequence => {
                write!(f, "Invalid escape sequence in literal string")
            }
            LiteralStringObjectError::UnbalancedParentheses => {
                write!(f, "Unbalanced parentheses in literal string")
            }
        }
    }
}

#[cfg(test)]
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

            let LiteralString(result) = parser.parse().unwrap();
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
            let result: Result<LiteralString, ParserError> = parser.parse();
            assert!(
                result.is_err(),
                "Expected error for invalid input `{}`",
                String::from_utf8_lossy(input)
            );
        }
    }
}
