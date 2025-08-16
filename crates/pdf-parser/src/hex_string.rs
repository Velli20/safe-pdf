use pdf_tokenizer::{PdfToken, error::TokenizerError};
use thiserror::Error;

use crate::{PdfParser, traits::HexStringParser};

/// Represents an error that can occur while parsing a hex string object.
#[derive(Debug, PartialEq, Error)]
pub enum HexStringError {
    /// Indicates that the input contains a non-hexadecimal character.
    #[error("Invalid non-hex decimal character in the input: '{0}'")]
    NotHexDecimal(char),
    #[error("Tokenizer error: {0}")]
    TokenizerError(#[from] TokenizerError),
}

impl HexStringParser for PdfParser<'_> {
    type ErrorType = HexStringError;

    /// Parses a hexadecimal string object from the current position in the input stream.
    ///
    /// According to PDF 1.7 Specification (Section 7.3.4.3), a hex string:
    ///
    /// # Format
    ///
    /// - Must begin with a single `<` character and end with a single `>` character
    /// - Contains an even number of hexadecimal digits between the delimiters
    /// - Valid digits are: `0`-`9`, `a`-`f`, and `A`-`F` (case-insensitive)
    ///
    /// # Exampe Inputs
    ///
    /// ```text
    /// <4E6F762073686D6F7A206B6120706F702E>
    /// ```
    ///
    /// # Returns
    ///
    /// `String` containing the decoded string value or an error if invalid format
    /// or characters are encountered.
    fn parse_hex_string(&mut self) -> Result<Vec<u8>, Self::ErrorType> {
        self.tokenizer.expect(PdfToken::LeftAngleBracket)?;

        // 1. Read until the closing `>` of the hex string.
        let hex_string = self.tokenizer.read_while_u8(|c| c != b'>');

        let mut filtered = Vec::new();
        // 2. Filter out whitespace characters.
        for b in hex_string {
            if Self::id_pdf_whitespace(*b) {
                continue;
            }

            // 2. Check if the character is a valid hex digit (0-9, a-f, A-F)
            if !b.is_ascii_hexdigit() {
                return Err(HexStringError::NotHexDecimal(*b as char));
            }
            // 3. Append hex digits to the hex string.
            filtered.push(*b);
        }

        // 3. Handle odd number of hex digits (Appendix H, Implementer Note 5 for Section 7.3.4.3)
        // "If the string contains an odd number of hexadecimal digits, the last digit
        // shall be assumed to be 0."
        if filtered.len() % 2 != 0 {
            filtered.push(b'0');
        }

        // Consume the closing `>` of the hex string.
        self.tokenizer.expect(PdfToken::RightAngleBracket)?;

        // Convert hex string to bytes
        let bytes = filtered
            .chunks(2)
            .map(|chunk| {
                let hex = String::from_utf8_lossy(chunk);
                u8::from_str_radix(&hex, 16).unwrap_or(0)
            })
            .collect::<Vec<u8>>();

        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hex_string_valid() {
        let valid_inputs: Vec<(&[u8], &str)> = vec![
            (b"<4E6F762073686D6F7A206B6120706F702E>", "Nov shmoz ka pop."),
            (b"<48656C6C6F20576F726C64>", "Hello World"),
            (
                b"<4E6F762073686D6F7A206B6120706F702E  >",
                "Nov shmoz ka pop.",
            ),
            (
                b"<4E6F762073686D6F7A206B6120706F702E\n>",
                "Nov shmoz ka pop.",
            ),
            (
                b"<4E6F762073686D6F7A206B6120706F702E\t>",
                "Nov shmoz ka pop.",
            ),
            (
                b"<4E6F762073686D6F7A206B6120706F702E\r>",
                "Nov shmoz ka pop.",
            ),
            (
                b"<4E6F762073686D6F7A206B6120706F702E\x0C>",
                "Nov shmoz ka pop.",
            ),
            (b"<01>", "\u{1}"),
            (
                b"<E2BAA9>", // UTF-8 for U+2EA9
                "\u{2EA9}",  // "⺩"
            ),
        ];

        for (input, expected) in valid_inputs {
            let mut parser = PdfParser::from(input);
            let result = parser.parse_hex_string().unwrap();
            let result = String::from_utf8(result).unwrap();
            assert_eq!(result, expected);
        }
    }

    #[test]
    fn test_parse_hex_string_invalid() {
        let invalid_inputs: Vec<&[u8]> = vec![
            b"<4E6F762073686D6F7A206B6120706F702E",  // Missing closing '>'
            b"4E6F762073686D6F7A206B6120706F702E>",  // Missing opening '<'
            b"<4E6F762073686D6F7Z206B6120706F702E>", // Invalid hex character 'Z'
            b"<4E6F762073686D6F7A206B6120706F702E>>", // Extra closing '>'
        ];

        for input in invalid_inputs {
            let mut parser = PdfParser::from(input);
            let result = parser.parse_hex_string();
            assert!(
                result.is_err(),
                "Expected error for invalid input `{}`",
                String::from_utf8_lossy(input)
            );
        }
    }
}
