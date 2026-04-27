use crate::{error::ParserError, parser::PdfParser};
use pdf_object::{dictionary::Dictionary, object_resolver::ObjectResolver};
use pdf_tokenizer::error::TokenizerError;

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

        let stream_data_start = self.tokenizer.position;
        let declared_stream_end = stream_data_start
            .saturating_add(length)
            .min(self.tokenizer.input.len());

        // Read the stream data
        let stream_data = match self.tokenizer.read_exactly(length) {
            Ok(stream_data) => stream_data.to_vec(),
            Err(TokenizerError::UnexpectedEndOfFile(_, _)) => {
                if let Some((recovered_data_end, recovered_endstream_offset)) =
                    find_nearby_endstream(
                        self.tokenizer.input,
                        stream_data_start,
                        declared_stream_end,
                    )
                {
                    let recovered_data = self
                        .tokenizer
                        .input
                        .get(stream_data_start..recovered_data_end)
                        .unwrap_or(&[])
                        .to_vec();
                    self.tokenizer.position = recovered_endstream_offset;
                    self.read_keyword(STREAM_END)?;
                    return Ok(recovered_data);
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

        // Read the `endstream` keyword .
        if let Err(original_error) = self.read_keyword(STREAM_END) {
            if let Some((recovered_data_end, recovered_endstream_offset)) =
                find_nearby_endstream(self.tokenizer.input, stream_data_start, declared_stream_end)
            {
                let recovered_data = self
                    .tokenizer
                    .input
                    .get(stream_data_start..recovered_data_end)
                    .unwrap_or(&[])
                    .to_vec();
                self.tokenizer.position = recovered_endstream_offset;
                self.read_keyword(STREAM_END)?;
                return Ok(recovered_data);
            }

            return Err(original_error);
        }

        Ok(stream_data)
    }
}

fn find_nearby_endstream(
    input: &[u8],
    stream_data_start: usize,
    declared_stream_end: usize,
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
        let distance = endstream_offset.abs_diff(declared_stream_end);
        match best_candidate {
            Some((best_offset, _, best_distance))
                if best_distance < distance
                    || (best_distance == distance && best_offset <= endstream_offset) => {}
            _ => {
                best_candidate = Some((endstream_offset, stream_data_end, distance));
            }
        }
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
    fn test_parse_stream_recovers_incorrect_length_too_short() {
        let dictionary = Dictionary::new(
            vec![("Length".to_string(), ObjectVariant::Integer(5))] // Incorrect length
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
            vec![("Length".to_string(), ObjectVariant::Integer(53))]
                .into_iter()
                .collect(),
        );

        let input = b"stream\ntoString\nendstream\n";
        let mut parser = PdfParser::from(input.as_slice());

        let result = parser.parse_stream(&dictionary, &PassthroughResolver);
        assert_eq!(result.unwrap(), b"toString");
    }
}
