use crate::{error::ParserError, parser::PdfParser};
use pdf_object_reader::{
    dictionary::Dictionary, object_lookup::ObjectLookupExt, object_resolver::ObjectResolver,
};

impl PdfParser<'_> {
    /// Parses a PDF stream object from the input, using a pre-parsed dictionary.
    ///
    /// Reads the raw bytes of a stream body using the pre-parsed `dictionary` for metadata.
    ///
    /// Expects the input to be positioned at the `stream` keyword. Resolves `/Length`, reads
    /// exactly that many bytes, and then validates and consumes `endstream`. Filter decoding is
    /// the caller's responsibility.
    ///
    /// # Errors
    ///
    /// Returns an error if `/Length` is absent or cannot be resolved, the declared bytes are not
    /// available, or the `stream` or `endstream` keyword is missing or malformed.
    pub fn parse_stream(
        &mut self,
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Vec<u8>, ParserError> {
        const STREAM_START: &[u8] = b"stream";
        const STREAM_END: &[u8] = b"endstream";

        self.read_keyword(STREAM_START)?;
        let length = dictionary.required_number::<usize>(b"Length", objects)?;
        let stream_data = self.tokenizer.read_exactly(length)?.to_vec();
        self.try_read_end_of_line_marker();
        self.read_keyword(STREAM_END)?;

        Ok(stream_data)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::collections::BTreeMap;

    use pdf_object_reader::{object_resolver::PassthroughResolver, object_variant::ObjectVariant};

    use super::*;

    #[test]
    fn test_parse_stream_missing_stream_keyword() {
        let dictionary = Dictionary::new(
            vec![(Vec::from(b"Length"), ObjectVariant::Integer(11))]
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
            vec![(Vec::from(b"Length"), ObjectVariant::Integer(11))]
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
        let dictionary = Dictionary::new(BTreeMap::<Vec<u8>, ObjectVariant>::new());

        let input = b"stream\nHello World\nendstream";
        let mut parser = PdfParser::from(input.as_slice());

        let result = parser.parse_stream(&dictionary, &PassthroughResolver);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_stream_missing_length_entry_without_endstream_errors() {
        let dictionary = Dictionary::new(BTreeMap::<Vec<u8>, ObjectVariant>::new());

        let input = b"stream\nHello World\nendstrm";
        let mut parser = PdfParser::from(input.as_slice());

        let result = parser.parse_stream(&dictionary, &PassthroughResolver);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_stream_rejects_incorrect_length_too_short() {
        let dictionary = Dictionary::new(
            vec![(Vec::from(b"Length"), ObjectVariant::Integer(5))] // Incorrect length
                .into_iter()
                .collect(),
        );

        let input = b"stream\nHello World\nendstream";
        let mut parser = PdfParser::from(input.as_slice());

        let result = parser.parse_stream(&dictionary, &PassthroughResolver);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_stream_rejects_incorrect_length_too_long() {
        let dictionary = Dictionary::new(
            vec![(Vec::from(b"Length"), ObjectVariant::Integer(53))]
                .into_iter()
                .collect(),
        );

        let input = b"stream\ntoString\nendstream\n";
        let mut parser = PdfParser::from(input.as_slice());

        let result = parser.parse_stream(&dictionary, &PassthroughResolver);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_stream_preserves_embedded_terminator_bytes() {
        let payload = b"before\nendstream\nendobj\nafter";
        let dictionary = Dictionary::new(
            vec![(
                Vec::from(b"Length"),
                ObjectVariant::Integer(i64::try_from(payload.len()).unwrap()),
            )]
            .into_iter()
            .collect(),
        );
        let mut input = b"stream\n".to_vec();
        input.extend_from_slice(payload);
        input.extend_from_slice(b"\nendstream");
        let mut parser = PdfParser::from(input.as_slice());

        let result = parser
            .parse_stream(&dictionary, &PassthroughResolver)
            .unwrap();

        assert_eq!(result, payload);
    }
}
