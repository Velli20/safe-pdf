use std::ops::Range;

use crate::{error::ParserError, parser::PdfParser};

impl PdfParser<'_> {
    /// Locates a raw stream body of exactly `length` bytes.
    ///
    /// Expects the input to be positioned at the `stream` keyword. Skips exactly `length`
    /// bytes, then validates and consumes `endstream`. Returns the byte range of the body
    /// within the parser's input, so the caller can share the bytes rather than copy them.
    /// Filter decoding is the caller's responsibility.
    ///
    /// # Errors
    ///
    /// Returns an error if the declared bytes are not available, or the `stream` or
    /// `endstream` keyword is missing or malformed.
    pub(crate) fn parse_stream(&mut self, length: usize) -> Result<Range<usize>, ParserError> {
        const STREAM_START: &[u8] = b"stream";
        const STREAM_END: &[u8] = b"endstream";

        self.read_keyword(STREAM_START)?;
        let data_start = self.position();
        self.tokenizer.read_exactly(length)?;
        let data_end = self.position();
        self.try_read_end_of_line_marker();
        self.read_keyword(STREAM_END)?;

        Ok(data_start..data_end)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_stream_missing_stream_keyword() {
        let mut parser = PdfParser::from(b"strm\nHello World\nendstream".as_slice());
        assert!(parser.parse_stream(11).is_err());
    }

    #[test]
    fn test_parse_stream_missing_endstream_keyword() {
        let mut parser = PdfParser::from(b"stream\nHello World\nendstrm".as_slice());
        assert!(parser.parse_stream(11).is_err());
    }

    #[test]
    fn test_parse_stream_rejects_incorrect_length_too_short() {
        let mut parser = PdfParser::from(b"stream\nHello World\nendstream".as_slice());
        assert!(parser.parse_stream(5).is_err());
    }

    #[test]
    fn test_parse_stream_rejects_incorrect_length_too_long() {
        let mut parser = PdfParser::from(b"stream\ntoString\nendstream\n".as_slice());
        assert!(parser.parse_stream(53).is_err());
    }

    #[test]
    fn test_parse_stream_preserves_embedded_terminator_bytes() {
        let payload = b"before\nendstream\nendobj\nafter";
        let mut input = b"stream\n".to_vec();
        input.extend_from_slice(payload);
        input.extend_from_slice(b"\nendstream");
        let mut parser = PdfParser::from(input.as_slice());

        let range = parser.parse_stream(payload.len()).unwrap();

        assert_eq!(input.get(range).unwrap(), payload);
    }
}
