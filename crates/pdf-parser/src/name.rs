use pdf_tokenizer::{PdfToken, error::TokenizerError};
use thiserror::Error;

use crate::{parser::PdfParser, traits::NameParser};

/// Represents an error that can occur while parsing a Name object.
#[derive(Debug, PartialEq, Error)]
pub enum NameObjectError {
    #[error(
        "Invalid hex escape in name object: Incomplete sequence, expected two hex digits after '#'"
    )]
    IncompleteHexEscape,
    #[error("Invalid hex escape in name object: Non-hex character '{0}' found in sequence")]
    NonHexDigitInEscape(char),
    #[error(
        "Invalid hex escape in name object: Could not parse hex string '{hex_pair}'. Reason: {reason}"
    )]
    HexRadixError { hex_pair: String, reason: String },
    #[error("Invalid token in name object (e.g., empty name after '/')")]
    InvalidToken,
    #[error("Tokenizer error: {0}")]
    TokenizerError(#[from] TokenizerError),
}

impl NameParser for PdfParser<'_> {
    type ErrorType = NameObjectError;

    /// Parses a PDF name object from the current position in the input stream.
    ///
    /// According to the PDF 1.7 Specification (Section 7.3.5 "Name Objects"):
    /// A name object is an atomic symbol uniquely defined by a sequence of characters.
    ///
    /// # Format
    ///
    /// - Must begin with a solidus character (`/`). The solidus is a prefix and not
    ///   part of the name itself.
    /// - The sequence of characters following the solidus forms the name.
    /// - The name can include any regular characters. Regular characters are any
    ///   characters except null (0x00), tab (0x09), line feed (0x0A), form feed (0x0C),
    ///   carriage return (0x0D), space (0x20), and the delimiter characters:
    ///   `( ) < > [ ] { } / %`.
    /// - Any character that is not a regular character (including space, delimiters,
    ///   or characters outside the printable ASCII range) must be encoded using a
    ///   number sign (`#`) followed by its two-digit hexadecimal code (e.g., `#20` for a space).
    /// - The name is terminated by any whitespace or delimiter character.
    /// - The maximum length of a name is 127 bytes. (This parser does not currently enforce this limit).
    ///
    /// # Example Inputs
    ///
    /// ```text
    /// /Name1
    /// /ASimpleName
    /// /NameWithSpaces#20Here
    /// /Path#2FTo#2FFile
    /// /A#25SymbolWithPercent
    /// /FontName#20#28Bold#29
    /// ```
    ///
    /// # Returns
    ///
    /// A `Name` object containing the decoded name string (with hex escapes resolved),
    /// or a `ParserError` if the input does not start with `/`, is empty after the `/`,
    /// or contains an invalid hex escape sequence.
    fn parse_name(&mut self) -> Result<Vec<u8>, Self::ErrorType> {
        self.tokenizer.expect(PdfToken::Solidus)?;

        let name = self.tokenizer.read_while_u8(|b| !Self::is_pdf_delimiter(b));
        if name.is_empty() {
            return Err(NameObjectError::InvalidToken);
        }

        escape(name)
    }
}

/// Decodes escape sequences in PDF name objects.
/// Handles '#' followed by two hex digits by converting them to the corresponding byte value.
fn escape(input: &[u8]) -> Result<Vec<u8>, NameObjectError> {
    let mut result = Vec::with_capacity(input.len());
    let mut iter = input.iter();

    while let Some(&byte) = iter.next() {
        match byte {
            b'#' => {
                let h1 = *iter.next().ok_or(NameObjectError::IncompleteHexEscape)?;
                let h2 = *iter.next().ok_or(NameObjectError::IncompleteHexEscape)?;

                if !h1.is_ascii_hexdigit() {
                    return Err(NameObjectError::NonHexDigitInEscape(char::from(h1)));
                }
                if !h2.is_ascii_hexdigit() {
                    return Err(NameObjectError::NonHexDigitInEscape(char::from(h2)));
                }

                let high = hex_digit_value(h1);
                let low = hex_digit_value(h2);
                result.push((high << 4) | low);
            }
            _ => result.push(byte),
        }
    }
    Ok(result)
}

/// Converts an ASCII hex digit to its numeric value (0–15).
/// Caller must ensure the byte is a valid hex digit.
const fn hex_digit_value(b: u8) -> u8 {
    match b {
        b'0'..=b'9' => b.saturating_sub(b'0'),
        b'a' => 10,
        b'b' => 11,
        b'c' => 12,
        b'd' => 13,
        b'e' => 14,
        b'f' => 15,
        b'A' => 10,
        b'B' => 11,
        b'C' => 12,
        b'D' => 13,
        b'E' => 14,
        b'F' => 15,
        _ => 0,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_name_object_valid() {
        let valid_inputs: Vec<(&[u8], &[u8])> = vec![
            (b"/Name\n", b"Name"),
            (b"/Name\t", b"Name"),
            (b"/Name1 ", b"Name1"),
            (b"/Name ", b"Name"),
            (b"/Name<", b"Name"),
            (b"/Name>", b"Name"),
            (b"/Name[", b"Name"),
            (b"/Name]", b"Name"),
            (b"/Name{", b"Name"),
            (b"/Name}", b"Name"),
            (b"/Name(abc)", b"Name"),
            (b"/Name", b"Name"),
            (b"/A#20Name", b"A Name"),
            (b"/D#23E#5fF", b"D#E_F"),
            (b"/A#20B", b"A B"),
        ];
        for (input, expected) in valid_inputs {
            let mut parser = PdfParser::from(input);
            let value = parser.parse_name().unwrap();
            assert_eq!(
                value,
                expected,
                "input: `{}`",
                String::from_utf8_lossy(input)
            );
        }
    }

    #[test]
    fn test_name_object_invalid() {
        let invalid_inputs: Vec<&[u8]> = vec![
            b"Name",     // Missing leading '/'
            b"/Name#",   // '#' at the end, no hex digits
            b"/Name#2",  // Only one hex digit after '#'
            b"/Name#ZZ", // Invalid hex digits after '#'
            //b"/Name\0WithNull", // Null byte in name
            b"/#",       // '#' without hex digits
            b"/##",      // Double '#' with no valid escapes
            b"/Name#G1", // 'G' is not a valid hex digit
        ];
        for input in invalid_inputs {
            let mut parser = PdfParser::from(input);
            let result = parser.parse_name();
            if result.is_ok() {
                panic!(
                    "Expected error for input `{}`",
                    String::from_utf8_lossy(input)
                );
            }
            assert!(result.is_err());
        }
    }
}
