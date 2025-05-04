use pdf_object::{dictionary::Dictionary, stream::Stream};
use pdf_tokenizer::PdfToken;

use crate::{PdfParser, StreamObjectParser, error::ParserError};

/// Represents an error that can occur while parsing an indirect object or an object reference.
#[derive(Debug, PartialEq)]
pub enum StreamParsingError {
    /// Indicates that the keyword `stream` is invalid.
    InvalidStreamKeyword(String),
    /// Indicates that the keyword `endstream` is invalid.
    InvalidEndStreamKeyword(String),
    /// Indicates that there was an error while parsing the stream.
    StreamParsingError(String),
    MissingLength,
}

impl<'a> StreamObjectParser for PdfParser<'a> {
    /// Parses a PDF stream object from the current position in the input buffer.
    ///
    /// A stream object consists of a dictionary followed by the keywords `stream` and `endstream`,
    /// with a sequence of bytes (the stream data) in between. The dictionary describes
    /// properties of the stream, such as its length and any applied filters.
    ///
    /// The stream data is returned as-is, without applying any decoding filters.
    ///
    /// See PDF 1.7 Specification, Sections 7.3.8 and 7.3.8.2 for the full definition of
    /// stream objects.
    ///
    /// All streams shall be indirect objects (see 7.3.10, "Indirect Objects") and the stream dictionary shall
    /// be a direct object. The keyword stream that follows the stream dictionary shall be followed by an
    /// end-of-line marker consisting of either a CARRIAGE RETURN and a LINE FEED or just a LINE FEED,
    /// and not by a CARRIAGE RETURN alone. The sequence of bytes that make up a stream lie between the
    /// end-of-line marker following the stream keyword and the endstream keyword; the stream dictionary
    /// specifies the exact number of bytes. There should be an end-of-line marker after the data and
    /// before endstream; this marker shall not be included in the stream length. There shall not be any
    /// extra bytes, other than white space, between endstream and endobj.
    ///
    /// # Parameters
    ///
    /// - `dictionary`: A reference to the dictionary object that describes the stream.
    fn parse_stream(&mut self, dictionary: &Dictionary) -> Result<Stream, ParserError> {
        const STREAM_START: &[u8] = b"stream";
        const STREAM_END: &[u8] = b"endstream";

        // Read the `stream` keyword .
        self.read_keyword(STREAM_START)?;

        // Find the length of the stream.
        let length = dictionary
            .get_number("Length")
            .ok_or(ParserError::from(StreamParsingError::MissingLength))?;

        // Find the decode type of the stream.
        // let decode = dictionary
        //     .get_string("Filter")
        //     .ok_or(ParserError::StreamParsingError(
        //         "Stream dictionary missing /Filter entry".to_string(),
        //     ))?;

        // Read the stream data
        let stream_data = self.tokenizer.read_excactly(length as usize)?.to_vec();

        // There should be an end-of-line marker after the data and before `endstream``
        self.tokenizer.expect(PdfToken::NewLine)?;

        // Read the `endstream` keyword .
        self.read_keyword(STREAM_END)?;

        Ok(Stream::new(stream_data))
    }
}

impl std::fmt::Display for StreamParsingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StreamParsingError::InvalidStreamKeyword(keyword) => {
                write!(f, "Expexted `stream` kyword, got: '{}'", keyword)
            }
            StreamParsingError::InvalidEndStreamKeyword(keyword) => {
                write!(f, "Expexted `endstream` kyword, got: '{}'", keyword)
            }
            StreamParsingError::StreamParsingError(error) => {
                write!(f, "Error while parsing stream: {}", error)
            }
            StreamParsingError::MissingLength => {
                write!(f, "Stream dictionary missing /Length entry")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use pdf_object::{Value, number::Number};

    use super::*;

    #[test]
    fn test_parse_stream_valid() {
        let dictionary = Dictionary::new(
            vec![(
                "Length".to_string(),
                Box::new(Value::Number(Number::new(11))),
            )]
            .into_iter()
            .collect(),
        );

        let input = b"stream\nHello World\nendstream";
        let mut parser = PdfParser::from(input.as_slice());

        let result = parser.parse_stream(&dictionary).unwrap();
        assert_eq!(result.data, b"Hello World");
    }

    #[test]
    fn test_parse_stream_missing_stream_keyword() {
        let dictionary = Dictionary::new(
            vec![(
                "Length".to_string(),
                Box::new(Value::Number(Number::new(11))),
            )]
            .into_iter()
            .collect(),
        );

        let input = b"strm\nHello World\nendstream";
        let mut parser = PdfParser::from(input.as_slice());

        let result = parser.parse_stream(&dictionary);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_stream_missing_endstream_keyword() {
        let dictionary = Dictionary::new(
            vec![(
                "Length".to_string(),
                Box::new(Value::Number(Number::new(11))),
            )]
            .into_iter()
            .collect(),
        );

        let input = b"stream\nHello World\nendstrm";
        let mut parser = PdfParser::from(input.as_slice());

        let result = parser.parse_stream(&dictionary);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_stream_missing_length_entry() {
        let dictionary = Dictionary::new(BTreeMap::new());

        let input = b"stream\nHello World\nendstream";
        let mut parser = PdfParser::from(input.as_slice());

        let result = parser.parse_stream(&dictionary);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_stream_incorrect_length() {
        let dictionary = Dictionary::new(
            vec![(
                "Length".to_string(),
                Box::new(Value::Number(Number::new(5))),
            )] // Incorrect length
            .into_iter()
            .collect(),
        );

        let input = b"stream\nHello World\nendstream";
        let mut parser = PdfParser::from(input.as_slice());

        let result = parser.parse_stream(&dictionary);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_stream_with_extra_whitespace() {
        let dictionary = Dictionary::new(
            vec![(
                "Length".to_string(),
                Box::new(Value::Number(Number::new(11))),
            )]
            .into_iter()
            .collect(),
        );

        let input = b"stream\n   Hello World   \nendstream";
        let mut parser = PdfParser::from(input.as_slice());

        let result = parser.parse_stream(&dictionary);
        assert!(result.is_err()); // Extra whitespace should cause an error
    }
}
