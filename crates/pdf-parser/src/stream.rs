use std::io::Read;

use flate2::bufread::ZlibDecoder;
use pdf_object::dictionary::Dictionary;
use pdf_tokenizer::{PdfToken, error::TokenizerError};
use thiserror::Error;

use crate::{PdfParser, error::ParserError, traits::StreamParser};

/// Represents an error that can occur while parsing an indirect object or an object reference.
#[derive(Debug, PartialEq, Error)]
pub enum StreamParsingError {
    /// Indicates an error while parsing the 'stream' keyword.
    #[error("Failed to parse 'stream' keyword: {source}")]
    InvalidStreamKeyword {
        #[source]
        source: ParserError,
    },
    /// Indicates an error while parsing the 'endstream' keyword.
    #[error("Failed to parse 'endstream' keyword: {source}")]
    InvalidEndStreamKeyword {
        #[source]
        source: ParserError,
    },
    /// Indicates that there was an error while parsing the stream.
    #[error("Error while parsing stream: {0}")]
    StreamParsingError(String),
    /// Indicates that the stream dictionary is missing the /Length entry.
    #[error("Stream dictionary missing /Length entry")]
    MissingLength,
    /// Indicates that the stream compression algorithm specified in the
    /// stream dictionary is not supported by the parser.
    #[error("Unsupported stream filter: {0}")]
    UsupportedFilter(String),
    /// Indicates that there was an error while decoding the stream data.
    #[error("Error while decoding stream: {0}")]
    DecompressionError(String),
    #[error("Tokenizer error: {0}")]
    TokenizerError(#[from] TokenizerError),
    #[error("Parser error: {0}")]
    ParserError(#[from] ParserError),
}

impl StreamParser for PdfParser<'_> {
    type ErrorType = StreamParsingError;

    /// Parses a PDF stream object from the input, using a pre-parsed dictionary.
    ///
    /// According to the PDF 1.7 Specification (Section 7.3.8 "Stream Objects"):
    /// A stream object, like a string object, is a sequence of bytes. However, PDF
    /// applications can read a stream incrementally, while a string must be read in
    /// its entirety. Furthermore, a stream can be of unlimited length, whereas a
    /// string is subject to an implementation limit.
    ///
    /// # Format
    ///
    /// - A stream consists of a dictionary followed by the keyword `stream`, then an
    ///   end-of-line (EOL) marker, a sequence of bytes (the stream data), another
    ///   EOL marker, and finally the keyword `endstream` followed by its EOL marker.
    /// - The EOL marker is typically a carriage return and a line feed (CRLF) or just a
    ///   line feed (LF). The parser's `read_keyword` helper handles EOLs after keywords.
    ///   An explicit EOL check is made after the stream data and before `endstream`.
    /// - The stream's dictionary (which must be parsed *before* calling this function
    ///   and is passed as an argument) provides metadata about the stream.
    /// - **Required Dictionary Entries for this Parser:**
    ///   - `/Length`: An integer specifying the exact number of bytes in the raw
    ///     stream data (i.e., the data between the EOL after `stream` and the EOL
    ///     before `endstream`).
    ///   - `/Filter`: A name (e.g., `/FlateDecode`) or an array of names specifying
    ///     the decoding filter(s) to be applied. This parser currently requires this
    ///     entry and only supports a single `/FlateDecode` filter.
    ///
    /// The expected sequence of tokens and data is:
    /// `stream<EOL_after_keyword>...data_bytes...<EOL_before_endstream>endstream<EOL_after_keyword>`
    ///
    /// # Decoding and Implementation Notes
    ///
    /// - This function is called when the parser expects a stream object, immediately
    ///   after its associated dictionary has been parsed.
    /// - It consumes the `stream` keyword and its trailing EOL.
    /// - It reads exactly `/Length` bytes from the input as the raw stream data.
    /// - It expects and consumes an EOL marker immediately after the raw stream data.
    /// - It consumes the `endstream` keyword and its trailing EOL.
    /// - If the `/Filter` entry in the dictionary is `/FlateDecode`, the raw stream
    ///   data is decompressed using Zlib (DEFLATE).
    /// - **Current Limitation**: Only `/FlateDecode` is supported. If `/Filter` is
    ///   missing or specifies an unsupported filter, an error is returned.
    ///
    /// # Example Input
    ///
    /// Assuming the dictionary `<< /Length L /Filter /FlateDecode >>` has been parsed,
    /// and `L` is the length of the *compressed* data:
    /// ```text
    /// stream
    /// ... (L bytes of Flate-compressed data) ...
    /// endstream
    /// ```
    ///
    /// # Returns
    ///
    /// - `Ok(Vec<u8>)`: A vector containing the decoded stream data.
    /// - `Err(ParserError)`: If keywords are missing/malformed, EOL markers are not
    ///   found where expected, required dictionary entries (`/Length`, `/Filter`) are
    ///   missing, the specified `/Filter` is unsupported, or a decompression error occurs.
    fn parse_stream(&mut self, dictionary: &Dictionary) -> Result<Vec<u8>, Self::ErrorType> {
        const STREAM_START: &[u8] = b"stream";
        const STREAM_END: &[u8] = b"endstream";

        // Read the `stream` keyword .
        self.read_keyword(STREAM_START)
            .map_err(|source| StreamParsingError::InvalidStreamKeyword { source })?;

        // Find the length of the stream.
        let length = dictionary
            .get_number("Length")
            .ok_or(StreamParsingError::MissingLength)?;

        // Find the decode type of the stream.
        let decode = dictionary.get_string("Filter");

        // Read the stream data
        let stream_data = self.tokenizer.read_excactly(length as usize)?.to_vec();

        // There should be an end-of-line marker after the data and before `endstream`.
        if let Some(PdfToken::CarriageReturn) = self.tokenizer.peek() {
            let _ = self.tokenizer.read();
        }

        self.read_end_of_line_marker()?;

        // Read the `endstream` keyword .
        self.read_keyword(STREAM_END)
            .map_err(|source| StreamParsingError::InvalidEndStreamKeyword { source })?;

        // Check if the stream data is compressed using the FlateDecode (DEFLATE) algorithm.
        if let Some(decode) = decode {
            if decode == "FlateDecode" {
                let mut d = ZlibDecoder::new(stream_data.as_slice());
                let mut s = Vec::new();

                if let Err(e) = d.read_to_end(&mut s) {
                    return Err(StreamParsingError::DecompressionError(e.to_string()));
                }

                return Ok(s);
            } else if decode == "DCTDecode" {
                // let mut decoder = zune_jpeg::JpegDecoder::new(stream_data.as_slice());
                // let s = decoder
                //     .decode()
                //     .map_err(|source| StreamParsingError::DecompressionError(source.to_string()))?;

                return Ok(stream_data.to_vec());
            }

            return Err(StreamParsingError::UsupportedFilter(decode.to_string()));
        }
        Ok(stream_data)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use pdf_object::ObjectVariant;

    use super::*;

    #[test]
    fn test_parse_stream_missing_stream_keyword() {
        let dictionary = Dictionary::new(
            vec![("Length".to_string(), Box::new(ObjectVariant::Integer(11)))]
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
            vec![("Length".to_string(), Box::new(ObjectVariant::Integer(11)))]
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
            vec![("Length".to_string(), Box::new(ObjectVariant::Integer(5)))] // Incorrect length
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
            vec![("Length".to_string(), Box::new(ObjectVariant::Integer(11)))]
                .into_iter()
                .collect(),
        );

        let input = b"stream\n   Hello World   \nendstream";
        let mut parser = PdfParser::from(input.as_slice());

        let result = parser.parse_stream(&dictionary);
        assert!(result.is_err()); // Extra whitespace should cause an error
    }
}
