use pdf_object::hex_string::HexString;
use pdf_tokenizer::PdfToken;

use crate::{ParseObject, PdfParser, error::ParserError};

/// Represents an error that can occur while parsing a hex string object.
#[derive(Debug, PartialEq)]
pub enum HexStringError {
    /// Indicates that the input contains a non-hexadecimal character.
    NotHexadecimal(char),
}

impl ParseObject<HexString> for PdfParser<'_> {
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
    /// `HexString` containing the decoded string value or an error if invalid format
    /// or characters are encountered.
    fn parse(&mut self) -> Result<HexString, ParserError> {
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
                return Err(ParserError::HexStringError(HexStringError::NotHexadecimal(
                    *b as char,
                )));
            }
            // 3. Append hex digits to the hex string.
            filtered.push(*b);
        }

        // 3. Handle odd number of hex digits (Appendix H, Implementer Note 5 for Section 7.3.4.3)
        // "If the string contains an odd number of hexadecimal digits, the last digit
        // shall be assumed to be 0."
        if filtered.len() % 2 != 0 {
            filtered.push('0' as u8);
        }

        // Consume the closing `>` of the hex string.
        self.tokenizer.expect(PdfToken::RightAngleBracket)?;

        // Convert hex string to bytes
        let hex_string = filtered
            .chunks(2)
            .map(|chunk| {
                let hex = String::from_utf8_lossy(chunk);
                u8::from_str_radix(&hex, 16).unwrap_or(0)
            })
            .collect::<Vec<u8>>();
        // Convert to a string
        let hex_string = String::from_utf8(hex_string).unwrap_or_default();

        // Return the hex string as a Value
        Ok(HexString::new(hex_string))
    }
}

impl std::fmt::Display for HexStringError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HexStringError::NotHexadecimal(c) => {
                write!(f, "Invalid non-hex decimal character in the input: '{}'", c)
            }
        }
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
        ];

        for (input, expected) in valid_inputs {
            let mut parser = PdfParser::from(input);
            let result = parser.parse().unwrap();
            let HexString(value) = result;
            assert_eq!(value, expected);
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
            let result: Result<HexString, ParserError> = parser.parse();
            assert!(
                result.is_err(),
                "Expected error for invalid input `{}`",
                String::from_utf8_lossy(input)
            );
        }
    }
}
