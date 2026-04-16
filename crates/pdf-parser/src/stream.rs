use crate::{error::ParserError, parser::PdfParser};
use pdf_object::{dictionary::Dictionary, object_resolver::ObjectResolver};

impl PdfParser<'_> {
    /// Parses a PDF stream object from the input, using a pre-parsed dictionary.
    ///
    /// Reads the raw bytes of a stream body using the pre-parsed `dictionary` for metadata.
    ///
    /// Expects the input to be positioned at the `stream` keyword. Reads exactly `/Length`
    /// bytes, then validates and consumes the surrounding `stream`/`endstream` keywords and
    /// EOL markers. Filter decoding is the caller's responsibility.
    ///
    /// # Errors
    ///
    /// Returns an error if the `stream` or `endstream` keyword is missing or malformed,
    /// an EOL marker is absent where required, or the `/Length` entry is missing from
    /// the dictionary.
    pub fn parse_stream(
        &mut self,
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Vec<u8>, ParserError> {
        const STREAM_START: &[u8] = b"stream";
        const STREAM_END: &[u8] = b"endstream";

        // Read the `stream` keyword .
        self.read_keyword(STREAM_START)?;

        // Find the length of the stream.
        let length = dictionary
            .get_or_err("Length")?
            .try_number::<usize>(objects)?;

        // Read the stream data
        let stream_data = self.tokenizer.read_exactly(length)?.to_vec();

        // There should be an end-of-line marker after the data and before `endstream`.
        self.try_read_end_of_line_marker();

        // Read the `endstream` keyword .
        self.read_keyword(STREAM_END)?;

        Ok(stream_data)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::collections::BTreeMap;

    use pdf_object::{object_resolver::PassthroughResolver, object_variant::ObjectVariant};

    use super::*;

    #[test]
    fn test_parse_stream_missing_stream_keyword() {
        let dictionary = Dictionary::new(
            vec![("Length".to_string(), ObjectVariant::Integer(11))]
                .into_iter()
                .collect(),
        );

        let input = b"strm\nHello World\nendstream";
        let mut parser = PdfParser::from(input.as_slice());

        let result = parser.parse_stream(&dictionary, &PassthroughResolver);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_stream_missing_endstream_keyword() {
        let dictionary = Dictionary::new(
            vec![("Length".to_string(), ObjectVariant::Integer(11))]
                .into_iter()
                .collect(),
        );

        let input = b"stream\nHello World\nendstrm";
        let mut parser = PdfParser::from(input.as_slice());

        let result = parser.parse_stream(&dictionary, &PassthroughResolver);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_stream_missing_length_entry() {
        let dictionary = Dictionary::new(BTreeMap::new());

        let input = b"stream\nHello World\nendstream";
        let mut parser = PdfParser::from(input.as_slice());

        let result = parser.parse_stream(&dictionary, &PassthroughResolver);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_stream_incorrect_length() {
        let dictionary = Dictionary::new(
            vec![("Length".to_string(), ObjectVariant::Integer(5))] // Incorrect length
                .into_iter()
                .collect(),
        );

        let input = b"stream\nHello World\nendstream";
        let mut parser = PdfParser::from(input.as_slice());

        let result = parser.parse_stream(&dictionary, &PassthroughResolver);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_stream_with_extra_whitespace() {
        let dictionary = Dictionary::new(
            vec![("Length".to_string(), ObjectVariant::Integer(11))]
                .into_iter()
                .collect(),
        );

        let input = b"stream\n   Hello World   \nendstream";
        let mut parser = PdfParser::from(input.as_slice());

        let result = parser.parse_stream(&dictionary, &PassthroughResolver);
        assert!(result.is_err()); // Extra whitespace should cause an error
    }
}
