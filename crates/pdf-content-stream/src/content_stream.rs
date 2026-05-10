use crate::{error::PdfOperatorError, pdf_operator::PdfOperatorVariant};
use pdf_object::{
    dictionary::Dictionary, error::ObjectError, object_resolver::ObjectResolver,
    object_variant::ObjectVariant, stream::StreamObject,
};

/// Allocates monotonically increasing IDs for parsed [`ContentStream`] values.
#[derive(Debug, Default)]
pub struct ContentStreamIdAllocator {
    next_id: usize,
}

impl ContentStreamIdAllocator {
    /// Creates a new allocator whose first issued ID is `0`.
    pub const fn new() -> Self {
        Self { next_id: 0 }
    }

    /// Returns the next content-stream ID.
    pub fn next_id(&mut self) -> Result<usize, PdfOperatorError> {
        let Some(next_id) = self.next_id.checked_add(1) else {
            return Err(PdfOperatorError::ContentStreamIdExhausted);
        };

        let id = self.next_id;
        self.next_id = next_id;
        Ok(id)
    }
}

/// Represents the content stream of a PDF page, containing a sequence
/// of drawing operators.
pub struct ContentStream {
    /// The parsed drawing operators from the content stream.
    pub operators: Vec<PdfOperatorVariant>,
    /// A monotonic ID assigned when this content stream is materialized.
    pub id: usize,
}

/// Processes an array of PDF objects, each expected to be a stream or reference to a stream,
/// and concatenates their content stream operators into a single vector.
///
/// # Parameters
///
/// - `array`: Slice of PDF objects representing streams or references to streams.
/// - `objects`: Resolver for indirect PDF objects.
///
/// # Returns
///
/// Concatenated list of operators or error.
fn process_content_stream_array(
    array: &[ObjectVariant],
    objects: &dyn ObjectResolver,
) -> Result<Vec<PdfOperatorVariant>, PdfOperatorError> {
    let mut combined_data = Vec::new();

    for value_in_array in array.iter() {
        let data = value_in_array.try_stream(objects)?.data()?;
        if !combined_data.is_empty() {
            // Separate adjacent stream payloads so tokens do not merge accidentally.
            combined_data.push(b'\n');
        }
        combined_data.extend_from_slice(&data);
    }

    PdfOperatorVariant::parse(&combined_data)
}

impl ContentStream {
    /// Constructs a [`ContentStream`] from a PDF page dictionary by resolving the `/Contents` entry.
    ///
    /// The `/Contents` entry may be a stream or an array of streams. This function resolves the entry,
    /// parses the content stream operators, and returns a [`ContentStream`] containing all operators.
    ///
    /// # Parameters
    ///
    /// - `dictionary`: The page dictionary containing the `/Contents` entry.
    /// - `objects`: Resolver for indirect PDF objects.
    /// - `id_allocator`: Monotonic allocator used to assign the returned content stream ID.
    ///
    /// # Returns
    ///
    /// The parsed content stream or None if missing.
    pub fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
        id_allocator: &mut ContentStreamIdAllocator,
    ) -> Result<Option<ContentStream>, PdfOperatorError> {
        const KEY: &str = "Contents";

        // Get the optional `/Contents` entry from the page dictionary.
        let Some(contents) = dictionary.get(KEY) else {
            return Ok(None);
        };

        // Process the resolved /Contents object.
        // It should be a Stream or an Array whose payload is one of these.
        let operators = match objects.resolve_object(contents)? {
            ObjectVariant::Stream(stream) => {
                let data = stream.data()?;
                PdfOperatorVariant::parse(&data)?
            }
            ObjectVariant::Array(array_obj) => {
                // The /Contents entry is an array of streams.
                process_content_stream_array(array_obj, objects)?
            }
            other => {
                return Err(ObjectError::TypeMismatch("Stream or Array", other.name()).into());
            }
        };

        let id = id_allocator.next_id()?;
        Ok(Some(ContentStream { operators, id }))
    }

    pub fn from_stream(
        stream: &StreamObject,
        id_allocator: &mut ContentStreamIdAllocator,
    ) -> Result<Self, PdfOperatorError> {
        let data = stream.data()?;
        let operators = PdfOperatorVariant::parse(&data)?;
        let id = id_allocator.next_id()?;
        Ok(ContentStream { operators, id })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use pdf_object::{
        dictionary::Dictionary, object_resolver::PassthroughResolver,
        object_variant::ObjectVariant, stream::StreamObject,
    };

    use super::{ContentStream, ContentStreamIdAllocator};
    use crate::pdf_operator::PdfOperatorVariant;

    fn stream_object(object_number: usize, data: &[u8]) -> StreamObject {
        StreamObject::new(
            object_number,
            0,
            Box::new(Dictionary::new(BTreeMap::new())),
            data.to_vec(),
        )
    }

    #[test]
    fn missing_contents_does_not_consume_an_id() {
        let page = Dictionary::new(BTreeMap::new());
        let mut ids = ContentStreamIdAllocator::new();

        let contents = ContentStream::from_dictionary(&page, &PassthroughResolver, &mut ids)
            .expect("missing /Contents should not error");

        assert!(contents.is_none());

        let stream = stream_object(10, b"q");
        let content_stream =
            ContentStream::from_stream(&stream, &mut ids).expect("stream should parse");
        assert_eq!(content_stream.id, 0);
    }

    #[test]
    fn contents_array_flattens_into_one_stream_and_one_id() {
        let contents = ObjectVariant::Array(vec![
            ObjectVariant::Stream(stream_object(1, b"q")),
            ObjectVariant::Stream(stream_object(2, b"Q")),
        ]);
        let page = Dictionary::new(BTreeMap::from([("Contents".to_string(), contents)]));
        let mut ids = ContentStreamIdAllocator::new();

        let content_stream = ContentStream::from_dictionary(&page, &PassthroughResolver, &mut ids)
            .expect("/Contents array should parse")
            .expect("page should have a content stream");

        assert_eq!(content_stream.id, 0);
        assert_eq!(content_stream.operators.len(), 2);

        let direct_stream = stream_object(3, b"q");
        let next_stream =
            ContentStream::from_stream(&direct_stream, &mut ids).expect("stream should parse");
        assert_eq!(next_stream.id, 1);
    }

    #[test]
    fn contents_array_concatenates_streams_before_parsing() {
        let contents = ObjectVariant::Array(vec![
            ObjectVariant::Stream(stream_object(1, b"0 j 0 J [")),
            ObjectVariant::Stream(stream_object(2, b"]0 d")),
        ]);
        let page = Dictionary::new(BTreeMap::from([("Contents".to_string(), contents)]));
        let mut ids = ContentStreamIdAllocator::new();

        let content_stream = ContentStream::from_dictionary(&page, &PassthroughResolver, &mut ids)
            .expect("/Contents array should parse")
            .expect("page should have a content stream");

        assert_eq!(content_stream.id, 0);
        assert_eq!(content_stream.operators.len(), 3);
        assert!(matches!(
            content_stream.operators.first(),
            Some(PdfOperatorVariant::SetLineJoinStyle(_))
        ));
        assert!(matches!(
            content_stream.operators.get(1),
            Some(PdfOperatorVariant::SetLineCapStyle(_))
        ));
        assert!(matches!(
            content_stream.operators.get(2),
            Some(PdfOperatorVariant::SetDashPattern(_))
        ));
    }
}
