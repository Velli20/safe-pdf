use pdf_object_reader::{object_resolver::ObjectResolver, pdf_array::PdfArray};
use pdf_tokenizer::PdfToken;

use crate::{error::ParserError, parser::PdfParser};

impl PdfParser<'_> {
    /// Parses a PDF array object from the current position in the input stream.
    ///
    /// # Returns
    ///
    /// An `Array` object containing the parsed PDF objects as its elements,
    /// or a `ParserError` if the input is malformed (e.g., missing delimiters,
    /// invalid object syntax within the array, or an unexpected token).
    pub fn parse_array(&mut self, objects: &dyn ObjectResolver) -> Result<PdfArray, ParserError> {
        self.tokenizer.expect(PdfToken::LeftSquareBracket)?;
        self.skip_whitespace_and_comments();

        let mut values = Vec::new();
        loop {
            self.skip_whitespace_and_comments();

            match self.tokenizer.peek() {
                Some(PdfToken::RightSquareBracket) => break,
                Some(_) => values.push(self.parse_object(objects)?),
                None => break,
            };
        }

        self.tokenizer.expect(PdfToken::RightSquareBracket)?;

        Ok(PdfArray::from(values))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use pdf_object_reader::object_resolver::PassthroughResolver;

    use super::*;

    #[test]
    fn test_parse_array_valid() {
        let valid_inputs: Vec<(&[u8], usize)> = vec![
            (b"[]", 0),
            (b"[ ]", 0),
            (b"[\n]", 0),
            (b"[1 2 3]", 3),
            (b"[ 4 0 R]", 1),
            (b"[true false null]", 3),
            (b"[<4E6F762073686D6F7A206B6120706F702E> /Name]", 2),
            (b"[1.5 -2.3 0]", 3),
            (b"[<< /Key /Value >> (String)]", 2),
        ];

        for (input, expected_count) in valid_inputs {
            let mut parser = PdfParser::from(input);
            let result = parser.parse_array(&PassthroughResolver).unwrap();
            assert_eq!(
                result.len(),
                expected_count,
                "Expected {} elements, got {}",
                expected_count,
                result.len()
            );
        }

        let mut parser = PdfParser::from(b"[]0".as_slice());
        let result = parser.parse_array(&PassthroughResolver).unwrap();
        assert!(result.is_empty());
        assert_eq!(parser.tokenizer.peek_byte(), Some(b'0'));
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
            if let Ok(v) = parser.parse_array(&PassthroughResolver) {
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
