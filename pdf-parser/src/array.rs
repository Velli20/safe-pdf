use pdf_object::array::Array;
use pdf_tokenizer::PdfToken;

use crate::{ParseObject, PdfParser, error::ParserError};

/// Represents an error that can occur while parsing an array object.
#[derive(Debug, PartialEq)]
pub enum ArrayError {
    /// Indicates that there was an error while parsing an object within the array.
    InvalidObject(String),
}

impl ParseObject<Array> for PdfParser<'_> {
    /// Parses a PDF array object from the current position in the input stream.
    ///
    /// According to the PDF 1.7 Specification (Section 7.3.6 "Array Objects"):
    /// An array object is a one-dimensional collection of objects arranged sequentially.
    ///
    /// # Format
    ///
    /// - Must begin with a left square bracket `[` and end with a right square bracket `]`.
    /// - Contains zero or more PDF objects as its elements.
    /// - Elements can be any valid PDF object type (e.g., numbers, strings, names,
    ///   dictionaries, other arrays, booleans, null, or indirect references).
    /// - Elements are separated by whitespace. Whitespace is generally ignored between tokens.
    ///
    /// # Example Inputs
    ///
    /// ```text
    /// []
    /// [1 2 3]
    /// [ /Name (String) 12.3 true null ]
    /// [ [1 2] << /Key /Value >> ]
    /// [ 549 3.14 false /SomeName (This is a string.) ]
    /// ```
    ///
    /// # Returns
    ///
    /// An `Array` object containing the parsed PDF objects as its elements,
    /// or a `ParserError` if the input is malformed (e.g., missing delimiters,
    /// invalid object syntax within the array, or an unexpected token).
    fn parse(&mut self) -> Result<Array, ParserError> {
        self.tokenizer.expect(PdfToken::LeftSquareBracket)?;
        self.skip_whitespace();

        let mut values = Vec::new();
        while let Some(token) = self.tokenizer.peek()? {
            self.skip_whitespace();

            if let PdfToken::RightSquareBracket = token {
                break;
            }

            match self.parse_object() {
                Ok(value) => {
                    values.push(value);
                }
                Err(err) => {
                    return Err(ParserError::ArrayError(ArrayError::InvalidObject(format!(
                        "Invalid object in array: {:?}",
                        err
                    ))));
                }
            }

            if let Some(PdfToken::RightSquareBracket) = self.tokenizer.peek()? {
                break;
            }
            self.skip_whitespace();
        }

        self.tokenizer.expect(PdfToken::RightSquareBracket)?;

        Ok(Array::new(values))
    }
}

impl std::fmt::Display for ArrayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ArrayError::InvalidObject(err) => {
                write!(f, "Error while parsing array object: {}", err)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_array_valid() {
        let valid_inputs: Vec<(&[u8], usize)> = vec![
            (b"[1 2 3]", 3),
            (b"[ 4 0 R]", 1),
            (b"[true false null]", 3),
            (b"[<4E6F762073686D6F7A206B6120706F702E> /Name]", 2),
            (b"[1.5 -2.3 0]", 3),
            (b"[<< /Key /Value >> (String)]", 2),
        ];

        for (input, expected_count) in valid_inputs {
            let mut parser = PdfParser::from(input);
            let result: Array = parser.parse().unwrap();
            assert_eq!(
                result.0.len(),
                expected_count,
                "Expected {} elements, got {}",
                expected_count,
                result.0.len()
            );
        }
    }

    #[test]
    fn test_parse_array_invalid() {
        let invalid_inputs: Vec<&[u8]> = vec![
            b"[1 2 3",              // Missing closing ']'
            b"1 2 3]",              // Missing opening '['
            b"[1 2 invalid_token]", // Invalid token inside array
        ];

        for input in invalid_inputs {
            let mut parser = PdfParser::from(input);
            if let Ok(Array(v)) = parser.parse() {
                panic!(
                    "Expected Err, got {:?} len {} input '{}™",
                    v,
                    v.len(),
                    String::from_utf8_lossy(input)
                );
            }
        }
    }
}
