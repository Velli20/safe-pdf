use std::collections::BTreeMap;

use pdf_object::{dictionary::Dictionary, object_resolver::ObjectResolver};
use pdf_tokenizer::PdfToken;

use crate::{error::ParserError, parser::PdfParser};

enum DictionaryTerminator<'a> {
    DoubleRightAngleBracket,
    Keyword(&'a [u8]),
}

impl PdfParser<'_> {
    /// Parses a PDF dictionary object from the current position in the input stream.
    ///
    /// # Returns
    ///
    /// A `Dictionary` object containing the parsed key-value pairs,
    /// or a `ParserError` if the input is malformed.
    pub fn parse_dictionary(
        &mut self,
        objects: &dyn ObjectResolver,
    ) -> Result<Dictionary, ParserError> {
        // Expect the opening `<<` of the dictionary.
        self.tokenizer.expect(PdfToken::DoubleLeftAngleBracket)?;

        self.skip_whitespace_and_comments();

        let dictionary = self.parse_dictionary_entries_until(
            objects,
            DictionaryTerminator::DoubleRightAngleBracket,
        )?;

        // Consume the closing `>>` of the dictionary.
        self.tokenizer.expect(PdfToken::DoubleRightAngleBracket)?;

        Ok(dictionary)
    }

    /// Parses a dictionary entry sequence terminated by the given keyword, then consumes it.
    ///
    /// This is used by inline image parsing, where `ID` ends the dictionary and starts data.
    pub fn parse_dictionary_until_keyword(
        &mut self,
        objects: &dyn ObjectResolver,
        keyword: &[u8],
    ) -> Result<Dictionary, ParserError> {
        self.parse_dictionary_until_keyword_with_options(objects, keyword, true)
    }

    /// Parses a dictionary entry sequence terminated by the given keyword, then consumes it.
    ///
    /// Callers can opt out of trailing-EOL consumption when the keyword's following
    /// separator belongs to the next grammar production, such as inline-image data.
    pub(crate) fn parse_dictionary_until_keyword_with_options(
        &mut self,
        objects: &dyn ObjectResolver,
        keyword: &[u8],
        consume_trailing_eol: bool,
    ) -> Result<Dictionary, ParserError> {
        let dictionary =
            self.parse_dictionary_entries_until(objects, DictionaryTerminator::Keyword(keyword))?;
        self.read_keyword_with_optional_eol(keyword, consume_trailing_eol)?;

        Ok(dictionary)
    }

    fn parse_dictionary_entries_until(
        &mut self,
        objects: &dyn ObjectResolver,
        terminator: DictionaryTerminator<'_>,
    ) -> Result<Dictionary, ParserError> {
        let mut dictionary = BTreeMap::new();

        loop {
            self.skip_whitespace_and_comments();

            if self.tokenizer.peek().is_none() {
                return Err(ParserError::UnexpectedEndOfFile);
            }

            if terminator.is_reached(self) {
                return Ok(Dictionary::new(dictionary));
            }

            // Parse key. Dictionary keys are always ASCII per spec; convert at boundary.
            let key = String::from_utf8_lossy(&self.parse_name()?).into_owned();

            self.skip_whitespace_and_comments();

            // Parse object.
            let object = self.parse_object(objects)?;

            dictionary.insert(key, object);
        }
    }
}

impl DictionaryTerminator<'_> {
    fn is_reached(&self, parser: &mut PdfParser<'_>) -> bool {
        match self {
            Self::DoubleRightAngleBracket => {
                matches!(
                    parser.tokenizer.peek(),
                    Some(PdfToken::DoubleRightAngleBracket)
                )
            }
            Self::Keyword(keyword) => has_keyword_ahead(parser, keyword),
        }
    }
}

fn has_keyword_ahead(parser: &mut PdfParser<'_>, keyword: &[u8]) -> bool {
    let mark = parser.tokenizer.position;
    let is_match = parser.read_keyword(keyword).is_ok();
    parser.tokenizer.position = mark;
    is_match
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use pdf_object::object_resolver::PassthroughResolver;

    use super::*;

    #[test]
    fn test_dictionary_valid() {
        let inputs: Vec<(&[u8], usize)> = vec![
            (b"<< >>", 0),
            (b"<< /Type /Catalog >>", 1),
            (b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>", 3),
            (
                b"<< /Type /Annot /Rect [100 100 200 200] /A << /S /URI /URI (https://example.com) >> >>",
                3,
            ),
            (b"<< /Author (John Doe) /IsDraft true >>", 2),
            (b"<< /Count 42 /ID <4FAE23> >>", 2),
        ];

        for (input, expected_count) in inputs {
            let mut parser = PdfParser::from(input);
            let result = parser.parse_dictionary(&PassthroughResolver).unwrap();

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
    fn test_dictionary_until_keyword_valid() {
        let mut parser = PdfParser::from(b"/IM true /W 1 /H 1 ID \x00\x01".as_slice());
        let result = parser
            .parse_dictionary_until_keyword(&PassthroughResolver, b"ID")
            .unwrap();

        assert_eq!(result.dictionary.len(), 3);
        assert!(parser.tokenizer.data().starts_with(b" \x00\x01"));
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
            let result = parser.parse_dictionary(&PassthroughResolver);

            assert!(
                result.is_err(),
                "Expected Err for input '{}', got {:?}",
                String::from_utf8_lossy(input),
                result
            );
        }
    }

    #[test]
    fn test_dictionary_until_keyword_invalid_key() {
        let mut parser = PdfParser::from(b"(Title) /Something ID".as_slice());
        let result = parser.parse_dictionary_until_keyword(&PassthroughResolver, b"ID");

        assert!(result.is_err());
    }

    #[test]
    fn test_dictionary_until_keyword_missing_value() {
        let mut parser = PdfParser::from(b"/Title ID".as_slice());
        let result = parser.parse_dictionary_until_keyword(&PassthroughResolver, b"ID");

        assert!(result.is_err());
    }
}
