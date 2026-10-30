use crate::{error::ParserError, name::hex_digit_value, parser::PdfParser};
use pdf_object_reader::inline_bytes::InlineBytes;
use pdf_tokenizer::PdfToken;

impl PdfParser<'_> {
    /// Parses a hexadecimal string object from the current position in the input stream.
    ///
    /// # Returns
    ///
    /// The decoded bytes, or an error if the string is unterminated or contains a
    /// character that is neither a hex digit nor whitespace.
    pub fn parse_hex_string(&mut self) -> Result<InlineBytes, ParserError> {
        self.tokenizer.expect(PdfToken::LeftAngleBracket)?;

        // Read until the closing `>` of the hex string; whitespace between digits is ignored.
        let hex_string = self.tokenizer.read_while_u8(|c| c != b'>');
        let mut digits = hex_string
            .iter()
            .copied()
            .filter(|&byte| !Self::is_pdf_whitespace(byte));
        if let Some(invalid) = digits.clone().find(|byte| !byte.is_ascii_hexdigit()) {
            return Err(ParserError::NotHexDecimal(char::from(invalid)));
        }

        // Consume the closing `>` of the hex string.
        self.tokenizer.expect(PdfToken::RightAngleBracket)?;

        // If the string contains an odd number of hex digits, the last digit is assumed to be 0.
        Ok(std::iter::from_fn(|| {
            let high = digits.next()?;
            let low = digits.next().unwrap_or(b'0');
            Some((hex_digit_value(high) << 4) | hex_digit_value(low))
        })
        .collect())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
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
            let result = String::from_utf8(result.to_vec()).unwrap();
            assert_eq!(result, expected);
        }
    }

    #[test]
    fn test_parse_hex_string_invalid() {
        let invalid_inputs: Vec<&[u8]> = vec![
            b"<4E6F762073686D6F7A206B6120706F702E",  // Missing closing '>'
            b"4E6F762073686D6F7A206B6120706F702E>",  // Missing opening '<'
            b"<4E6F762073686D6F7Z206B6120706F702E>", // Invalid hex character 'Z'
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
