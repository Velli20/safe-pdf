use std::collections::BTreeMap;

use pdf_object::{dictionary::Dictionary, object_resolver::ObjectResolver};
use pdf_tokenizer::PdfToken;

use crate::{
    error::ParserError,
    parser::PdfParser,
    traits::{DictionaryParser, NameParser},
};

impl DictionaryParser for PdfParser<'_> {
    type ErrorType = ParserError;

    /// Parses a PDF dictionary object from the current position in the input stream.
    ///
    /// According to the PDF 1.7 Specification (Section 7.3.7 "Dictionary Objects"):
    /// A dictionary object is an associative table containing pairs of objects, known as
    /// the dictionary's entries.
    ///
    /// # Format
    ///
    /// - Must begin with double left angle brackets (`<<`) and end with double right
    ///   angle brackets (`>>`).
    /// - Contains zero or more key-value pairs.
    /// - Each key must be a PDF Name object (e.g., `/Type`, `/Size`).
    /// - Each value can be any valid PDF object (e.g., another dictionary, an array,
    ///   a number, a string, a name, a boolean, null, or an indirect reference).
    /// - Whitespace is generally ignored between tokens within the dictionary.
    ///
    /// # Example Inputs
    ///
    /// ```text
    /// << >>
    /// << /Type /Catalog /Pages 2 0 R >>
    /// << /Key1 (Value1) /Key2 123 /Key3 [1 2 3] >>
    /// << /NestedDict << /InnerKey /InnerValue >> >>
    /// ```
    ///
    /// # Returns
    ///
    /// A `Dictionary` object containing the parsed key-value pairs,
    /// or a `ParserError` if the input is malformed (e.g., missing delimiters,
    /// invalid key or value syntax, or an unexpected token).
    fn parse_dictionary(
        &mut self,
        objects: &dyn ObjectResolver,
    ) -> Result<Dictionary, Self::ErrorType> {
        // Expect the opening `<<` of the dictionary.
        self.tokenizer.expect(PdfToken::DoubleLeftAngleBracket)?;

        self.skip_whitespace_and_comments();

        let mut dictionary = BTreeMap::new();

        while let Some(token) = self.tokenizer.peek() {
            if let PdfToken::DoubleRightAngleBracket = token {
                break;
            }

            self.skip_whitespace_and_comments();

            // Parse key. Dictionary keys are always ASCII per spec; convert at boundary.
            let key = String::from_utf8_lossy(&self.parse_name()?).into_owned();

            self.skip_whitespace_and_comments();

            // Parse object.
            let object = self.parse_object(objects)?;

            dictionary.insert(key, object);
            self.skip_whitespace_and_comments();
        }

        // Consume the closing `>>` of the dictionary.
        self.tokenizer.expect(PdfToken::DoubleRightAngleBracket)?;

        Ok(Dictionary::new(dictionary))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use pdf_object::object_resolver::UnimplementedResolver;

    use super::*;

    #[test]
    fn test_dictionary_valid() {
        let inputs:  Vec<(&[u8], usize)> = vec![
            (b"<< >>", 0),
            (b"<< /Type /Catalog >>", 1),
            (b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>", 3),
            (b"<< /Type /Annot /Rect [100 100 200 200] /A << /S /URI /URI (https://example.com) >> >>", 3),
            (b"<< /Author (John Doe) /IsDraft true >>", 2),
            (b"<< /Count 42 /ID <4FAE23> >>", 2),
        ];

        for (input, expected_count) in inputs {
            let mut parser = PdfParser::from(input);
            let result = parser.parse_dictionary(&UnimplementedResolver).unwrap();

            assert_eq!(
                result.dictionary.len(),
                expected_count,
                "Expected {} elements for input '{}', got {}",
                expected_count,
                String::from_utf8_lossy(input),
                result.dictionary.len()
            );
        }
    }

    #[test]
    fn test_dictionary_invalid() {
        let inputs: Vec<&[u8]> = vec![
            // Missing closing >>
            b"<< /Type /Page",
            // No leading <<
            b"/Type /Page >>",
            // Invalid key format
            b"<< Type /Page >>",
            // Non-name as key
            b"<< (Title) /Something >>",
            // Unterminated string
            b"<< /Title (Missing end >>",
            // Unexpected object inside dictionary
            b"<< /Stream stream... endstream >>",
            // Invalid hex string
            b"<< /ID <Z23G> >>",
        ];

        for input in inputs {
            let mut parser = PdfParser::from(input);
            let result = parser.parse_dictionary(&UnimplementedResolver);

            assert!(
                result.is_err(),
                "Expected Err for input '{}', got {:?}",
                String::from_utf8_lossy(input),
                result
            );
        }
    }
}
