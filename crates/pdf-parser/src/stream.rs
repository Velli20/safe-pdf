use crate::{error::ParserError, parser::PdfParser};
use pdf_object::{
    dictionary::Dictionary, object_lookup::ObjectLookupExt, object_resolver::ObjectResolver,
};
use pdf_tokenizer::error::TokenizerError;

impl PdfParser<'_> {
    /// Parses a PDF stream object from the input, using a pre-parsed dictionary.
    ///
    /// Reads the raw bytes of a stream body using the pre-parsed `dictionary` for metadata.
    ///
    /// Expects the input to be positioned at the `stream` keyword. When `/Length` is present,
    /// reads exactly that many bytes and then validates and consumes the surrounding
    /// `stream`/`endstream` keywords and EOL markers. When `/Length` is missing, scans forward
    /// to the first valid `endstream` delimiter and recovers the bytes before it. Filter
    /// decoding is the caller's responsibility.
    ///
    /// # Errors
    ///
    /// Returns an error if the `stream` or `endstream` keyword is missing or malformed,
    /// an EOL marker is absent where required, or a missing `/Length` cannot be recovered
    /// by scanning to a valid `endstream` terminator.
    pub fn parse_stream(
        &mut self,
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Vec<u8>, ParserError> {
        const STREAM_START: &[u8] = b"stream";

        // Read the `stream` keyword .
        self.read_keyword(STREAM_START)?;

        let stream_data_start = self.tokenizer.position;
        let length = dictionary.optional_number::<usize>(b"Length", objects)?;

        match length {
            Some(length) => self.parse_stream_with_length(stream_data_start, length),
            None => self.parse_stream_without_length(stream_data_start),
        }
    }

    /// Parses a stream body when `/Length` is present in the stream dictionary.
    ///
    /// The parser first attempts the exact-length read path. If the declared length
    /// is wrong or the stream terminator is malformed, it falls back to the existing
    /// recovery scan that looks for a nearby valid `endstream` delimiter.
    fn parse_stream_with_length(
        &mut self,
        stream_data_start: usize,
        length: usize,
    ) -> Result<Vec<u8>, ParserError> {
        const STREAM_END: &[u8] = b"endstream";

        let declared_stream_end = stream_data_start
            .saturating_add(length)
            .min(self.tokenizer.input.len());

        // Read the stream data.
        let stream_data = match self.tokenizer.read_exactly(length) {
            Ok(stream_data) => stream_data.to_vec(),
            Err(TokenizerError::UnexpectedEndOfFile(_, _)) => {
                if let Some((recovered_data_end, recovered_endstream_offset)) = find_stream_end(
                    self.tokenizer.input,
                    stream_data_start,
                    Some(declared_stream_end),
                ) {
                    return self.finish_recovered_stream(
                        stream_data_start,
                        recovered_data_end,
                        recovered_endstream_offset,
                        STREAM_END,
                    );
                }

                return Err(TokenizerError::UnexpectedEndOfFile(
                    length,
                    self.tokenizer.input.len().saturating_sub(stream_data_start),
                )
                .into());
            }
            Err(error) => return Err(error.into()),
        };

        // There should be an end-of-line marker after the data and before `endstream`.
        self.try_read_end_of_line_marker();

        // Read the `endstream` keyword.
        if let Err(original_error) = self.read_keyword(STREAM_END) {
            if let Some((recovered_data_end, recovered_endstream_offset)) = find_stream_end(
                self.tokenizer.input,
                stream_data_start,
                Some(declared_stream_end),
            ) {
                return self.finish_recovered_stream(
                    stream_data_start,
                    recovered_data_end,
                    recovered_endstream_offset,
                    STREAM_END,
                );
            }

            return Err(original_error);
        }

        Ok(stream_data)
    }

    /// Parses a stream body when `/Length` is missing from the stream dictionary.
    ///
    /// The parser scans forward for the first valid `endstream` delimiter, trims any
    /// trailing end-of-line marker before it, and returns the recovered bytes. If no
    /// valid terminator is found, the parse fails with an EOF-style error.
    fn parse_stream_without_length(
        &mut self,
        stream_data_start: usize,
    ) -> Result<Vec<u8>, ParserError> {
        const STREAM_END: &[u8] = b"endstream";

        if let Some((recovered_data_end, recovered_endstream_offset)) =
            find_stream_end(self.tokenizer.input, stream_data_start, None)
        {
            return self.finish_recovered_stream(
                stream_data_start,
                recovered_data_end,
                recovered_endstream_offset,
                STREAM_END,
            );
        }

        Err(ParserError::UnexpectedEndOfFile)
    }

    /// Finishes a recovered stream by replacing the parser position with the recovered
    /// `endstream` offset, consuming the terminator, and returning the recovered bytes.
    ///
    /// This is shared by both the exact-length recovery path and the missing `/Length`
    /// scan path so they both normalize stream-body extraction the same way.
    fn finish_recovered_stream(
        &mut self,
        stream_data_start: usize,
        recovered_data_end: usize,
        recovered_endstream_offset: usize,
        stream_end_keyword: &[u8],
    ) -> Result<Vec<u8>, ParserError> {
        let recovered_data = self
            .tokenizer
            .input
            .get(stream_data_start..recovered_data_end)
            .unwrap_or(&[])
            .to_vec();
        self.tokenizer.position = recovered_endstream_offset;
        self.read_keyword(stream_end_keyword)?;
        Ok(recovered_data)
    }
}

fn find_stream_end(
    input: &[u8],
    stream_data_start: usize,
    declared_stream_end: Option<usize>,
) -> Option<(usize, usize)> {
    const STREAM_END: &[u8] = b"endstream";

    let mut best_candidate = None;

    for (relative_offset, window) in input
        .get(stream_data_start..)?
        .windows(STREAM_END.len())
        .enumerate()
    {
        if window != STREAM_END {
            continue;
        }

        let endstream_offset = stream_data_start.saturating_add(relative_offset);
        if let Some(next) = input
            .get(endstream_offset.saturating_add(STREAM_END.len()))
            .copied()
            && !PdfParser::is_pdf_delimiter(next)
        {
            continue;
        }

        let stream_data_end = trim_stream_data_end(input, stream_data_start, endstream_offset);
        if let Some(declared_stream_end) = declared_stream_end {
            let distance = endstream_offset.abs_diff(declared_stream_end);
            match best_candidate {
                Some((best_offset, _, best_distance))
                    if best_distance < distance
                        || (best_distance == distance && best_offset <= endstream_offset) => {}
                _ => {
                    best_candidate = Some((endstream_offset, stream_data_end, distance));
                }
            }
            continue;
        }

        return Some((stream_data_end, endstream_offset));
    }

    best_candidate.map(|(endstream_offset, stream_data_end, _)| (stream_data_end, endstream_offset))
}

fn trim_stream_data_end(input: &[u8], stream_data_start: usize, endstream_offset: usize) -> usize {
    if endstream_offset <= stream_data_start {
        return endstream_offset;
    }

    let last_byte_offset = endstream_offset.saturating_sub(1);
    match input.get(last_byte_offset).copied() {
        Some(b'\n') => match last_byte_offset
            .checked_sub(1)
            .and_then(|idx| input.get(idx))
        {
            Some(b'\r') if last_byte_offset.saturating_sub(1) >= stream_data_start => {
                last_byte_offset.saturating_sub(1)
            }
            _ => last_byte_offset,
        },
        Some(b'\r') => last_byte_offset,
        _ => endstream_offset,
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
        assert_eq!(result.unwrap(), b"Hello World");
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
    fn test_parse_stream_recovers_incorrect_length_too_short() {
        let dictionary = Dictionary::new(
            vec![(Vec::from(b"Length"), ObjectVariant::Integer(5))] // Incorrect length
                .into_iter()
                .collect(),
        );

        let input = b"stream\nHello World\nendstream";
        let mut parser = PdfParser::from(input.as_slice());

        let result = parser.parse_stream(&dictionary, &PassthroughResolver);
        assert_eq!(result.unwrap(), b"Hello World");
    }

    #[test]
    fn test_parse_stream_recovers_incorrect_length_too_long() {
        let dictionary = Dictionary::new(
            vec![(Vec::from(b"Length"), ObjectVariant::Integer(53))]
                .into_iter()
                .collect(),
        );

        let input = b"stream\ntoString\nendstream\n";
        let mut parser = PdfParser::from(input.as_slice());

        let result = parser.parse_stream(&dictionary, &PassthroughResolver);
        assert_eq!(result.unwrap(), b"toString");
    }
}
