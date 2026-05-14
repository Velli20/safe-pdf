use crate::{
    content_stream_id_allocator::ContentStreamIdAllocator,
    operator_stream_parser::OperatorStreamParser,
};
use pdf_content_stream_operators::{error::PdfOperatorError, variants::PdfOperatorVariant};
use pdf_object::{
    dictionary::Dictionary, error::ObjectError, object_resolver::ObjectResolver,
    object_variant::ObjectVariant, stream::StreamObject,
};

/// Represents one materialized PDF content stream as parsed operators plus its
/// stable content-stream ID.
pub struct ContentStream {
    /// The parsed drawing operators from the content stream.
    pub operators: Vec<PdfOperatorVariant>,
    /// A monotonic ID assigned when this content stream is materialized.
    pub id: usize,
}

impl ContentStream {
    /// Parses decoded content-stream bytes into a materialized content stream.
    ///
    /// The input is treated as one decoded PDF content stream. Parsing is
    /// tolerant of recoverable issues such as unknown operators and truncated
    /// trailing operands, matching the lower-level operator parser.
    ///
    /// A content-stream ID is allocated only after parsing succeeds. If
    /// parsing fails, the allocator is left unchanged.
    ///
    /// # Parameters
    ///
    /// - `input`: Decoded bytes of one PDF content stream.
    /// - `id_allocator`: Monotonic allocator used to assign the returned
    ///   content-stream ID.
    ///
    /// # Returns
    ///
    /// Returns a fully materialized [`ContentStream`] containing the parsed
    /// operators and a fresh ID.
    ///
    /// # Errors
    ///
    /// Returns [`PdfOperatorError`] if parsing fails or if the ID allocator is
    /// exhausted.
    pub fn new(
        input: &[u8],
        id_allocator: &mut ContentStreamIdAllocator,
    ) -> Result<Self, PdfOperatorError> {
        let mut operators = Vec::new();
        let mut parser = OperatorStreamParser::new(input, &mut operators);
        while parser.parse_next_item()? {}
        let id = id_allocator.next_id()?;
        Ok(Self { operators, id })
    }

    /// Parses an already resolved stream object into a materialized content stream.
    ///
    /// The stream payload is decoded through [`StreamObject::data`] and then
    /// passed to [`ContentStream::new`]. ID allocation therefore follows the
    /// same success-only semantics as [`ContentStream::new`].
    ///
    /// # Parameters
    ///
    /// - `stream`: Resolved PDF stream object to decode and parse.
    /// - `id_allocator`: Monotonic allocator used to assign the returned
    ///   content-stream ID.
    ///
    /// # Returns
    ///
    /// Returns a fully materialized [`ContentStream`] containing the parsed
    /// operators and a fresh ID.
    ///
    /// # Errors
    ///
    /// Returns [`PdfOperatorError`] if decoding the stream payload fails,
    /// parsing fails, or the ID allocator is exhausted.
    pub fn from_stream(
        stream: &StreamObject,
        id_allocator: &mut ContentStreamIdAllocator,
    ) -> Result<Self, PdfOperatorError> {
        let data = stream.data()?;
        Self::new(&data, id_allocator)
    }

    /// Resolves and parses an optional `/Contents` entry from a dictionary.
    ///
    /// This handles the PDF page/form `/Contents` forms that the codebase
    /// relies on:
    ///
    /// - missing `/Contents` returns `Ok(None)` without consuming an ID
    /// - a single stream is parsed directly
    /// - an array of streams is decoded, concatenated with one newline between
    ///   adjacent payloads, and parsed as a single logical stream
    /// - any other resolved type produces a type-mismatch error
    ///
    /// # Parameters
    ///
    /// - `dictionary`: Dictionary that may contain a `/Contents` entry.
    /// - `objects`: Object resolver used to materialize indirect references.
    /// - `id_allocator`: Monotonic allocator used to assign the returned
    ///   content-stream ID when content exists.
    ///
    /// # Returns
    ///
    /// Returns `Ok(None)` when `/Contents` is absent, or `Ok(Some(ContentStream))`
    /// when content exists and parses successfully.
    ///
    /// # Errors
    ///
    /// Returns [`PdfOperatorError`] if resolution, decoding, or parsing fails,
    /// or if `/Contents` resolves to a non-stream, non-array value.
    pub fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
        id_allocator: &mut ContentStreamIdAllocator,
    ) -> Result<Option<Self>, PdfOperatorError> {
        const KEY: &str = "Contents";

        let Some(contents) = dictionary.get(KEY) else {
            return Ok(None);
        };

        let content_stream = match objects.resolve_object(contents)? {
            ObjectVariant::Stream(stream) => Some(Self::from_stream(stream, id_allocator)?),
            ObjectVariant::Array(array_obj) => {
                let data = Self::concatenate_content_stream_array(array_obj, objects)?;
                Some(Self::new(&data, id_allocator)?)
            }
            other => {
                return Err(ObjectError::TypeMismatch("Stream or Array", other.name()).into());
            }
        };

        Ok(content_stream)
    }

    /// Concatenates an array of resolved content streams into one decoded byte
    /// buffer.
    ///
    /// Each element must resolve to a stream. The decoded payloads are appended
    /// in order with a single newline byte inserted between adjacent payloads.
    /// The separator prevents tokens at stream boundaries from merging when the
    /// source payloads do not already contain whitespace at the join point.
    ///
    /// # Parameters
    ///
    /// - `array`: Array of content-stream references to concatenate.
    /// - `objects`: Object resolver used to resolve each array entry into a
    ///   stream and decode its payload.
    ///
    /// # Returns
    ///
    /// Returns the concatenated decoded bytes for the array entries.
    ///
    /// # Errors
    ///
    /// Returns [`PdfOperatorError`] if any array element is not a stream, if a
    /// referenced stream cannot be decoded, or if object resolution fails.
    fn concatenate_content_stream_array(
        array: &[ObjectVariant],
        objects: &dyn ObjectResolver,
    ) -> Result<Vec<u8>, PdfOperatorError> {
        let mut combined_data = Vec::new();

        for value_in_array in array {
            let data = value_in_array.try_stream(objects)?.data()?;
            if !combined_data.is_empty() {
                combined_data.push(b'\n');
            }
            combined_data.extend_from_slice(&data);
        }

        Ok(combined_data)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use pdf_object::{
        dictionary::Dictionary, object_resolver::PassthroughResolver,
        object_variant::ObjectVariant, stream::StreamObject,
    };

    use super::ContentStream;

    #[test]
    fn concatenate_content_stream_array_inserts_newline_between_streams() {
        let array = [
            ObjectVariant::Stream(StreamObject::new(
                1,
                0,
                Box::new(Dictionary::new(BTreeMap::new())),
                b"q".to_vec(),
            )),
            ObjectVariant::Stream(StreamObject::new(
                2,
                0,
                Box::new(Dictionary::new(BTreeMap::new())),
                b"Q".to_vec(),
            )),
        ];

        let data = ContentStream::concatenate_content_stream_array(&array, &PassthroughResolver)
            .expect("array should concatenate");

        assert_eq!(data, b"q\nQ");
    }
}
