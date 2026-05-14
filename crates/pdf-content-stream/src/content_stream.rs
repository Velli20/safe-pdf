use crate::{
    content_stream_id_allocator::ContentStreamIdAllocator,
    operator_stream_parser::OperatorStreamParser,
};
use pdf_content_stream_operators::{error::PdfOperatorError, variants::PdfOperatorVariant};
use pdf_object::{
    dictionary::Dictionary, error::ObjectError, object_resolver::ObjectResolver,
    object_variant::ObjectVariant,
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
    /// Parses a resolved content-stream object into a materialized content stream.
    ///
    /// A single stream is decoded and parsed directly. An array is parsed by
    /// decoding and parsing each stream in order into the same operator buffer
    /// without concatenating the decoded bytes first.
    ///
    /// A content-stream ID is allocated only after parsing succeeds. If
    /// parsing fails, the allocator is left unchanged.
    ///
    /// # Parameters
    ///
    /// - `content`: A resolved stream or array of streams, optionally behind
    ///   an indirect reference.
    /// - `objects`: Object resolver used to follow indirect references.
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
        content: &ObjectVariant,
        objects: &dyn ObjectResolver,
        id_allocator: &mut ContentStreamIdAllocator,
    ) -> Result<Self, PdfOperatorError> {
        let mut operators = Vec::new();
        let resolved = objects.resolve_object(content)?;
        match resolved {
            ObjectVariant::Stream(stream) => {
                Self::parse_decoded_stream(stream.raw_data(), &mut operators)?;
            }
            ObjectVariant::Array(streams) => {
                for value in streams {
                    let stream = value.try_stream(objects)?;
                    Self::parse_decoded_stream(stream.raw_data(), &mut operators)?;
                }
            }
            other => {
                return Err(ObjectError::TypeMismatch("Stream or Array", other.name()).into());
            }
        }

        let id = id_allocator.next_id()?;
        Ok(Self { operators, id })
    }

    /// Parses decoded content-stream bytes into the supplied operator buffer.
    ///
    fn parse_decoded_stream(
        input: &[u8],
        operators: &mut Vec<PdfOperatorVariant>,
    ) -> Result<(), PdfOperatorError> {
        let mut parser = OperatorStreamParser::new(input, operators);
        while parser.parse_next_item()? {}
        Ok(())
    }

    /// Resolves and parses an optional `/Contents` entry from a dictionary.
    ///
    /// This handles the PDF page/form `/Contents` forms that the codebase
    /// relies on:
    ///
    /// - missing `/Contents` returns `Ok(None)` without consuming an ID
    /// - a single stream is parsed directly
    /// - an array of streams is parsed in order without concatenating the
    ///   decoded bytes first
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

        Ok(Some(Self::new(contents, objects, id_allocator)?))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::ContentStreamIdAllocator;
    use pdf_content_stream_operators::{
        TextElement,
        compatibility_operators::{BeginCompatibility, EndCompatibility},
        error::PdfOperatorError,
        graphics_state_operators::{RestoreGraphicsState, SaveGraphicsState},
        path_operators::{LineTo, MoveTo},
        recording_pdf_operator_backend::{RecordedOperation, RecordingBackend},
        text_showing_operators::ShowTextArray,
        variants::PdfOperatorVariant,
    };
    use pdf_object::{
        dictionary::Dictionary, error::ObjectError, object_resolver::ObjectResolver,
        object_resolver::PassthroughResolver, object_variant::ObjectVariant, stream::StreamObject,
    };

    use super::ContentStream;

    fn stream_object(object_number: usize, data: &[u8]) -> StreamObject {
        StreamObject::new(
            object_number,
            0,
            Box::new(Dictionary::new(BTreeMap::new())),
            data.to_vec(),
        )
    }

    struct MapResolver {
        objects: BTreeMap<usize, ObjectVariant>,
    }

    impl ObjectResolver for MapResolver {
        fn resolve_object<'a>(
            &'a self,
            obj: &'a ObjectVariant,
        ) -> Result<&'a ObjectVariant, ObjectError> {
            match obj {
                ObjectVariant::Reference(object_number) => self.objects.get(object_number).ok_or(
                    ObjectError::FailedResolveObjectReference {
                        obj_num: *object_number,
                    },
                ),
                _ => Ok(obj),
            }
        }
    }

    #[test]
    fn content_stream_new_returns_expected_operators_and_assigns_ids() {
        let mut ids = ContentStreamIdAllocator::new();
        let parsed = ContentStream::new(
            &ObjectVariant::Stream(stream_object(1, b"BX EX 10 20 m 30 40 l")),
            &PassthroughResolver,
            &mut ids,
        )
        .expect("stream should parse");

        assert_eq!(parsed.id, 0);
        assert_eq!(
            parsed.operators,
            vec![
                PdfOperatorVariant::BeginCompatibility(BeginCompatibility),
                PdfOperatorVariant::EndCompatibility(EndCompatibility),
                PdfOperatorVariant::MoveTo(MoveTo::new(10.0, 20.0)),
                PdfOperatorVariant::LineTo(LineTo::new(30.0, 40.0)),
            ]
        );
    }

    #[test]
    fn content_stream_new_handles_bare_sign_text_array_adjustment() {
        let mut ids = ContentStreamIdAllocator::new();
        let parsed = ContentStream::new(
            &ObjectVariant::Stream(stream_object(
                1,
                b"BT\n/F1 11.67 Tf\n1 0 0 1 10 20 Tm\n[(e)-4(x)12(t)-3(e)-4(n)-4(s)3(i)3(v)-(e)-4(l)3(y)]TJ\nET\n",
            )),
            &PassthroughResolver,
            &mut ids,
        )
        .expect("stream should parse");

        assert!(matches!(
            parsed.operators.get(3),
            Some(PdfOperatorVariant::ShowTextArray(op))
                if op
                    == &ShowTextArray::new(vec![
                        TextElement::Text { value: b"e".to_vec() },
                        TextElement::Adjustment { amount: -4.0 },
                        TextElement::Text { value: b"x".to_vec() },
                        TextElement::Adjustment { amount: 12.0 },
                        TextElement::Text { value: b"t".to_vec() },
                        TextElement::Adjustment { amount: -3.0 },
                        TextElement::Text { value: b"e".to_vec() },
                        TextElement::Adjustment { amount: -4.0 },
                        TextElement::Text { value: b"n".to_vec() },
                        TextElement::Adjustment { amount: -4.0 },
                        TextElement::Text { value: b"s".to_vec() },
                        TextElement::Adjustment { amount: 3.0 },
                        TextElement::Text { value: b"i".to_vec() },
                        TextElement::Adjustment { amount: 3.0 },
                        TextElement::Text { value: b"v".to_vec() },
                        TextElement::Adjustment { amount: 0.0 },
                        TextElement::Text { value: b"e".to_vec() },
                        TextElement::Adjustment { amount: -4.0 },
                        TextElement::Text { value: b"l".to_vec() },
                        TextElement::Adjustment { amount: 3.0 },
                        TextElement::Text { value: b"y".to_vec() },
                    ])
        ));
    }

    #[test]
    fn parsed_inline_image_can_be_dispatched() {
        let mut ids = ContentStreamIdAllocator::new();
        let parsed = ContentStream::new(
            &ObjectVariant::Stream(stream_object(1, b"BI /W 1 /H 1 ID \x00 EI")),
            &PassthroughResolver,
            &mut ids,
        )
        .expect("inline image should parse");
        let inline_image = match parsed.operators.first() {
            Some(PdfOperatorVariant::InlineImage(image)) => image.clone(),
            other => panic!("expected inline image, got {other:?}"),
        };

        let mut backend = RecordingBackend::default();
        parsed.operators[0]
            .call(&mut backend)
            .expect("dispatch should succeed");

        assert_eq!(
            backend.operations,
            vec![RecordedOperation::PaintInlineImage {
                image: inline_image,
            }]
        );
    }

    #[test]
    fn content_stream_new_skips_unknown_operator_and_recovers() {
        let mut ids = ContentStreamIdAllocator::new();
        let parsed = ContentStream::new(
            &ObjectVariant::Stream(stream_object(1, b"@ q")),
            &PassthroughResolver,
            &mut ids,
        )
        .expect("stream should parse");

        assert_eq!(
            parsed.operators,
            vec![PdfOperatorVariant::SaveGraphicsState(SaveGraphicsState)]
        );
    }

    #[test]
    fn content_stream_new_parses_array_streams_in_order_without_concatenation() {
        let contents = ObjectVariant::Array(vec![
            ObjectVariant::Reference(1),
            ObjectVariant::Reference(2),
        ]);
        let mut ids = ContentStreamIdAllocator::new();
        let resolver = MapResolver {
            objects: BTreeMap::from([
                (1, ObjectVariant::Stream(stream_object(1, b"1 2"))),
                (2, ObjectVariant::Stream(stream_object(2, b"3 4 m"))),
            ]),
        };

        let parsed = ContentStream::new(&contents, &resolver, &mut ids)
            .expect("contents array should parse");

        assert_eq!(parsed.id, 0);
        assert_eq!(
            parsed.operators,
            vec![PdfOperatorVariant::MoveTo(MoveTo::new(3.0, 4.0))]
        );
    }

    #[test]
    fn content_stream_new_rejects_non_stream_array_entries() {
        let contents = ObjectVariant::Array(vec![ObjectVariant::Null]);
        let mut ids = ContentStreamIdAllocator::new();

        let err = match ContentStream::new(&contents, &PassthroughResolver, &mut ids) {
            Ok(_) => panic!("non-stream array entries should fail"),
            Err(err) => err,
        };

        assert!(matches!(
            err,
            PdfOperatorError::Object(ObjectError::TypeMismatch("Stream", "Null"))
        ));
        assert_eq!(ids.next_id().expect("id should remain unconsumed"), 0);
    }

    #[test]
    fn content_stream_new_failure_does_not_consume_an_id() {
        let mut ids = ContentStreamIdAllocator::new();
        let err = match ContentStream::new(
            &ObjectVariant::Stream(stream_object(1, b"BI /W 1 /H 1 ID abc")),
            &PassthroughResolver,
            &mut ids,
        ) {
            Ok(_) => panic!("malformed inline image should fail"),
            Err(err) => err,
        };

        assert!(matches!(err, PdfOperatorError::ParserError(_)));
        assert_eq!(ids.next_id().expect("id should still be zero"), 0);
    }

    #[test]
    fn from_dictionary_preserves_allocator_for_missing_contents() {
        let page = Dictionary::new(BTreeMap::new());
        let mut ids = ContentStreamIdAllocator::new();

        let contents = ContentStream::from_dictionary(&page, &PassthroughResolver, &mut ids)
            .expect("missing contents should not error");

        assert!(contents.is_none());
        assert_eq!(ids.next_id().expect("id should still start at zero"), 0);
    }

    #[test]
    fn from_dictionary_parses_stream_arrays_and_allocates_monotonically() {
        let contents = ObjectVariant::Array(vec![
            ObjectVariant::Reference(1),
            ObjectVariant::Reference(2),
        ]);
        let page = Dictionary::new(BTreeMap::from([("Contents".to_string(), contents)]));
        let mut ids = ContentStreamIdAllocator::new();
        let resolver = MapResolver {
            objects: BTreeMap::from([
                (1, ObjectVariant::Stream(stream_object(1, b"q"))),
                (2, ObjectVariant::Stream(stream_object(2, b"Q"))),
            ]),
        };

        let content_stream = ContentStream::from_dictionary(&page, &resolver, &mut ids)
            .expect("contents array should parse")
            .expect("page should have a content stream");

        assert_eq!(content_stream.id, 0);
        assert_eq!(
            content_stream.operators,
            vec![
                PdfOperatorVariant::SaveGraphicsState(SaveGraphicsState),
                PdfOperatorVariant::RestoreGraphicsState(RestoreGraphicsState),
            ]
        );

        let next = ContentStream::new(
            &ObjectVariant::Stream(stream_object(3, b"q")),
            &resolver,
            &mut ids,
        )
        .expect("follow-up stream should parse");
        assert_eq!(next.id, 1);
    }

    #[test]
    fn content_stream_is_plain_data_after_parsing() {
        let content_stream = ContentStream {
            operators: vec![PdfOperatorVariant::SaveGraphicsState(SaveGraphicsState)],
            id: 9,
        };

        assert_eq!(content_stream.id, 9);
        assert_eq!(content_stream.operators.len(), 1);
    }
}
