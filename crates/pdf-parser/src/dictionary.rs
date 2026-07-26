use std::collections::BTreeMap;

use pdf_object::{
    dictionary::Dictionary, object_resolver::ObjectResolver, object_variant::ObjectVariant,
};
use pdf_tokenizer::PdfToken;

use crate::{error::ParserError, parser::PdfParser};

enum DictionaryEntryState {
    ExpectKeyOrTerminator,
    ExpectValue { key: String },
    ContinueNameValue { key: String, value: Vec<u8> },
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

        let dictionary = self.parse_dictionary_entries(objects)?;

        // Consume the closing `>>` of the dictionary.
        self.tokenizer.expect(PdfToken::DoubleRightAngleBracket)?;

        Ok(dictionary)
    }

    /// Parses the key-value pairs inside a regular dictionary until `>>`.
    fn parse_dictionary_entries(
        &mut self,
        objects: &dyn ObjectResolver,
    ) -> Result<Dictionary, ParserError> {
        let mut dictionary = BTreeMap::new();
        let mut state = DictionaryEntryState::ExpectKeyOrTerminator;

        loop {
            self.skip_whitespace_and_comments();

            if self.tokenizer.peek().is_none() {
                return Err(ParserError::UnexpectedEndOfFile);
            }

            if matches!(state, DictionaryEntryState::ExpectKeyOrTerminator)
                && self.is_at_dictionary_end()
            {
                return Ok(Dictionary::new(dictionary));
            }

            state = self.next_dictionary_state(objects, &mut dictionary, state)?;
        }
    }

    /// Parses the inline-image dictionary that precedes `ID`.
    ///
    /// This stops as soon as the next non-whitespace token is no longer a name
    /// key, leaving `ID` in the input stream for the caller to consume.
    pub(crate) fn parse_inline_image_dictionary(
        &mut self,
        objects: &dyn ObjectResolver,
    ) -> Result<Dictionary, ParserError> {
        let mut dictionary = BTreeMap::new();

        loop {
            self.skip_whitespace_and_comments();

            match self.tokenizer.peek() {
                Some(PdfToken::Solidus) => {}
                Some(_) => return Ok(Dictionary::new(dictionary)),
                None => return Err(ParserError::UnexpectedEndOfFile),
            }

            let key = String::from_utf8_lossy(&self.parse_name()?).into_owned();
            self.skip_whitespace_and_comments();
            let object = self.parse_inline_image_dictionary_value(objects)?;
            dictionary.insert(key, object);
        }
    }

    /// Parses a single inline-image dictionary value.
    ///
    /// Inline-image dictionaries allow bare names as values, so this preserves
    /// the leading slash form when the next token is another name.
    fn parse_inline_image_dictionary_value(
        &mut self,
        objects: &dyn ObjectResolver,
    ) -> Result<ObjectVariant, ParserError> {
        match self.tokenizer.peek() {
            Some(PdfToken::Solidus) => Ok(ObjectVariant::Name(self.parse_name()?)),
            _ => self.parse_object(objects),
        }
    }

    /// Advances the regular dictionary state machine by one step.
    fn next_dictionary_state(
        &mut self,
        objects: &dyn ObjectResolver,
        dictionary: &mut BTreeMap<String, ObjectVariant>,
        state: DictionaryEntryState,
    ) -> Result<DictionaryEntryState, ParserError> {
        match state {
            DictionaryEntryState::ExpectKeyOrTerminator => self.parse_dictionary_key_state(),
            DictionaryEntryState::ExpectValue { key } => {
                self.parse_dictionary_value_state(objects, dictionary, key)
            }
            DictionaryEntryState::ContinueNameValue { key, value } => {
                self.continue_dictionary_name_value(dictionary, key, value)
            }
        }
    }

    /// Reads the next dictionary key and transitions to value parsing.
    fn parse_dictionary_key_state(&mut self) -> Result<DictionaryEntryState, ParserError> {
        // Dictionary keys are always ASCII per spec; convert at boundary.
        let key = String::from_utf8_lossy(&self.parse_name()?).into_owned();
        Ok(DictionaryEntryState::ExpectValue { key })
    }

    /// Parses the next value for a regular dictionary entry.
    ///
    /// If the value starts with `/`, the parser keeps consuming adjacent regular
    /// characters so spaced name values remain intact.
    fn parse_dictionary_value_state(
        &mut self,
        objects: &dyn ObjectResolver,
        dictionary: &mut BTreeMap<String, ObjectVariant>,
        key: String,
    ) -> Result<DictionaryEntryState, ParserError> {
        match self.tokenizer.peek() {
            Some(PdfToken::Solidus) => {
                let value = self.parse_name()?;
                Ok(DictionaryEntryState::ContinueNameValue { key, value })
            }
            _ => {
                let object = self.parse_object(objects)?;
                dictionary.insert(key, object);
                Ok(DictionaryEntryState::ExpectKeyOrTerminator)
            }
        }
    }

    /// Extends a spaced name value until it reaches a real delimiter or key boundary.
    fn continue_dictionary_name_value(
        &mut self,
        dictionary: &mut BTreeMap<String, ObjectVariant>,
        key: String,
        mut value: Vec<u8>,
    ) -> Result<DictionaryEntryState, ParserError> {
        if self.is_at_dictionary_end() || matches!(self.tokenizer.peek(), Some(PdfToken::Solidus)) {
            dictionary.insert(key, ObjectVariant::Name(value));
            return Ok(DictionaryEntryState::ExpectKeyOrTerminator);
        }

        let Some(byte) = self.tokenizer.peek_byte() else {
            return Err(ParserError::UnexpectedEndOfFile);
        };

        if !Self::is_pdf_regular_character(byte) {
            dictionary.insert(key, ObjectVariant::Name(value));
            return Ok(DictionaryEntryState::ExpectKeyOrTerminator);
        }

        let suffix = self.tokenizer.read_while_u8(Self::is_pdf_regular_character);
        if !suffix.is_empty() {
            value.push(b' ');
            value.extend_from_slice(suffix);
        }

        Ok(DictionaryEntryState::ContinueNameValue { key, value })
    }

    /// Returns `true` when the next token closes the current dictionary.
    fn is_at_dictionary_end(&mut self) -> bool {
        matches!(
            self.tokenizer.peek(),
            Some(PdfToken::DoubleRightAngleBracket)
        )
    }
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
    fn test_dictionary_recovers_spaced_name_value() {
        let mut parser = PdfParser::from(b"<< /AnyName /A B /Next 1 >>".as_slice());
        let result = parser.parse_dictionary(&PassthroughResolver).unwrap();

        assert_eq!(
            result.get("AnyName"),
            Some(&ObjectVariant::Name(b"A B".to_vec()))
        );
        assert_eq!(result.get("Next"), Some(&ObjectVariant::Integer(1)));
    }

    #[test]
    fn test_dictionary_recovers_multiple_spaced_name_tokens() {
        let mut parser = PdfParser::from(b"<< /AnyName /A B C /Next 1 >>".as_slice());
        let result = parser.parse_dictionary(&PassthroughResolver).unwrap();

        assert_eq!(
            result.get("AnyName"),
            Some(&ObjectVariant::Name(b"A B C".to_vec()))
        );
        assert_eq!(result.get("Next"), Some(&ObjectVariant::Integer(1)));
    }

    #[test]
    fn test_dictionary_spaced_name_recovery_stops_at_next_key() {
        let mut parser = PdfParser::from(b"<< /Name /Value /Next /Other >>".as_slice());
        let result = parser.parse_dictionary(&PassthroughResolver).unwrap();

        assert_eq!(
            result.get("Name"),
            Some(&ObjectVariant::Name(b"Value".to_vec()))
        );
        assert_eq!(
            result.get("Next"),
            Some(&ObjectVariant::Name(b"Other".to_vec()))
        );
    }

    #[test]
    fn test_dictionary_rejects_bare_regular_token_after_non_name_value() {
        let mut parser = PdfParser::from(b"<< /Count 1 Foo /Next 2 >>".as_slice());
        let result = parser.parse_dictionary(&PassthroughResolver);

        assert!(result.is_err());
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
    fn test_inline_image_dictionary_stops_before_data_keyword() {
        let mut parser = PdfParser::from(b"/IM true /W 1 /H 1 ID \x00\x01".as_slice());
        let result = parser
            .parse_inline_image_dictionary(&PassthroughResolver)
            .unwrap();

        assert_eq!(result.dictionary.len(), 3);
        assert!(parser.tokenizer.data().starts_with(b"ID \x00\x01"));
    }

    #[test]
    fn test_inline_image_dictionary_name_value_stops_before_data_keyword() {
        let mut parser = PdfParser::from(b"/CS /DeviceGray ID \x00\x01".as_slice());
        let result = parser
            .parse_inline_image_dictionary(&PassthroughResolver)
            .unwrap();

        assert_eq!(
            result.get("CS"),
            Some(&ObjectVariant::Name(b"DeviceGray".to_vec()))
        );
        assert!(parser.tokenizer.data().starts_with(b"ID \x00\x01"));
    }

    #[test]
    fn test_inline_image_dictionary_missing_value_errors() {
        let mut parser = PdfParser::from(b"/Title ID".as_slice());
        let result = parser.parse_inline_image_dictionary(&PassthroughResolver);

        assert!(result.is_err());
    }
}
