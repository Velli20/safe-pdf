use pdf_tokenizer::{PdfToken, error::TokenizerError};
use thiserror::Error;

use crate::{PdfParser, traits::NameParser};

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
    fn parse_name(&mut self) -> Result<String, Self::ErrorType> {
        self.tokenizer.expect(PdfToken::Solidus)?;

        let name = self.tokenizer.read_while_u8(|b| !Self::is_pdf_delimiter(b));
        if name.is_empty() {
            return Err(NameObjectError::InvalidToken);
        }

        let name = escape(name)?;

        Ok(name)
    }
}

/// Decodes escape sequences in PDF name objects.
/// Handles '#' followed by two hex digits by converting them to the corresponding ASCII character.
fn escape(input: &[u8]) -> Result<String, NameObjectError> {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.iter();

    while let Some(byte) = chars.next() {
        match byte {
            b'#' => {
                // Read the first hex digit character
                let h1_byte = match chars.next() {
                    Some(b) => *b,
                    None => return Err(NameObjectError::IncompleteHexEscape),
                };
                // Read the second hex digit character
                let h2_byte = match chars.next() {
                    Some(b) => *b,
                    None => return Err(NameObjectError::IncompleteHexEscape),
                };

                let h1_char = h1_byte as char;
                let h2_char = h2_byte as char;

                if !h1_char.is_ascii_hexdigit() {
                    return Err(NameObjectError::NonHexDigitInEscape(h1_char));
                }
                if !h2_char.is_ascii_hexdigit() {
                    return Err(NameObjectError::NonHexDigitInEscape(h2_char));
                }

                let hex_pair_str = String::from_iter([h1_char, h2_char]);
                let byte_val = u8::from_str_radix(&hex_pair_str, 16).map_err(|e| {
                    NameObjectError::HexRadixError {
                        hex_pair: hex_pair_str,
                        reason: e.to_string(),
                    }
                })?;
                result.push(byte_val as char);
            }
            _ => result.push(*byte as char),
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name_object_valid() {
        let valid_inputs: Vec<(&[u8], &str)> = vec![
            (b"/Name\n", "Name"),
            (b"/Name\t", "Name"),
            (b"/Name1 ", "Name1"),
            (b"/Name ", "Name"),
            (b"/Name<", "Name"),
            (b"/Name>", "Name"),
            (b"/Name[", "Name"),
            (b"/Name]", "Name"),
            (b"/Name{", "Name"),
            (b"/Name}", "Name"),
            (b"/Name(abc)", "Name"),
            (b"/Name", "Name"),
            (b"/A#20Name", "A Name"),
            (b"/D#23E#5fF", "D#E_F"),
            (b"/A#20B", "A B"),
        ];
        for (input, expected) in valid_inputs {
            let mut parser = PdfParser::from(input);
            let value = parser.parse_name().unwrap();
            if value != expected {
                panic!(
                    "Expected `{}`, but got `{}` for input `{}`",
                    expected,
                    value,
                    String::from_utf8_lossy(input)
                );
            }
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
