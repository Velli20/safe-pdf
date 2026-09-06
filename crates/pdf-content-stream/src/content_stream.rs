use crate::operator_stream_parser::OperatorStreamParser;
use pdf_content_stream_operators::{error::PdfOperatorError, variants::PdfOperatorVariant};
use pdf_object_reader::{
    FromPdfObject, ObjectAccess, ObjectContext, ReadResult, object_kind::ObjectKind,
    stream::StreamObject,
};

/// Represents one materialized PDF content stream as parsed operators plus its
/// stable content-stream ID.
pub struct ContentStream {
    /// The parsed drawing operators from the content stream.
    pub operators: Vec<PdfOperatorVariant>,
    /// A monotonic ID assigned when this content stream is materialized.
    pub id: usize,
}

impl FromPdfObject for ContentStream {
    /// Parses a stream or an array of streams within the active object traversal.
    ///
    /// Malformed content syntax is parsed best-effort, retaining recovered operators.
    /// Object resolution and shape errors propagate, and consume no content-stream ID.
    /// Each successful read allocates one ID, including an empty stream array.
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut operators = Vec::new();
        let id = match context.object().kind() {
            ObjectKind::Array => {
                let mut context = context.array()?;
                for index in 0..context.array().len() {
                    let stream = context.at::<StreamObject>(index)?;
                    // A fresh parser keeps incomplete operands local to each stream.
                    Self::parse_decoded_stream(stream.raw_data(), &mut operators)?;
                }
                // The combined operator buffer receives one ID after every entry succeeds.
                context.content_stream_ids().next_id()?
            }
            _ => {
                let context = context.stream()?;
                Self::parse_decoded_stream(context.stream().raw_data(), &mut operators)?;
                context.content_stream_ids().next_id()?
            }
        };
        Ok(Self { operators, id })
    }
}

impl ContentStream {
    /// Parses bytes best-effort, preserving operators recovered around malformed syntax.
    fn parse_decoded_stream(
        input: &[u8],
        operators: &mut Vec<PdfOperatorVariant>,
    ) -> Result<(), PdfOperatorError> {
        let mut parser = OperatorStreamParser::new(input, operators);
        while parser.parse_next_item()? {}
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use pdf_content_stream_operators::{
        PdfTextItem,
        graphics_state_operators::SaveGraphicsState,
        recording_pdf_operator_backend::{RecordedOperation, RecordingBackend},
        text_showing_operators::ShowTextArray,
        variants::PdfOperatorVariant,
    };
    use pdf_object_reader::{
        dictionary::Dictionary, object_error::ObjectError, object_resolver::ObjectResolver,
        object_resolver::PassthroughResolver, object_variant::ObjectVariant, stream::StreamObject,
    };

    use super::ContentStream;

    struct PageContents(Option<ContentStream>);

    impl pdf_object_reader::FromPdfObject for PageContents {
        fn from_pdf_object(
            context: pdf_object_reader::ObjectContext<
                '_,
                impl pdf_object_reader::ObjectAccess + ?Sized,
            >,
        ) -> pdf_object_reader::ReadResult<Self> {
            Ok(Self(context.dictionary()?.optional(b"Contents")?))
        }
    }

    fn stream_object(object_number: usize, data: &[u8]) -> StreamObject {
        StreamObject::new(
            object_number,
            0,
            Dictionary::new(BTreeMap::<Vec<u8>, ObjectVariant>::new()),
            data.to_vec(),
        )
    }

    fn recorded_operations(operators: &[PdfOperatorVariant]) -> Vec<RecordedOperation> {
        let mut backend = RecordingBackend::default();
        for operator in operators {
            operator
                .call(&mut backend)
                .expect("operator should dispatch");
        }
        backend.operations
    }

    struct MapResolver {
        objects: BTreeMap<usize, ObjectVariant>,
    }

    impl pdf_object_reader::ObjectSource for MapResolver {
        type Error = std::convert::Infallible;

        fn read_object(
            &self,
            id: pdf_object_reader::object_id::ObjectId,
        ) -> Result<Option<pdf_object_reader::pdf_object::PdfObject>, Self::Error> {
            Ok(self
                .objects
                .get(&id.number)
                .cloned()
                .map(pdf_object_reader::pdf_object::PdfObject::new))
        }
    }

    impl ObjectResolver for MapResolver {
        fn resolve_object<'a>(
            &'a self,
            obj: &'a ObjectVariant,
        ) -> Result<&'a ObjectVariant, ObjectError> {
            match obj {
                ObjectVariant::Reference(object_number) => self
                    .objects
                    .get(&object_number.number)
                    .ok_or(ObjectError::FailedResolveObjectReference {
                        obj_num: object_number.number,
                    }),
                _ => Ok(obj),
            }
        }
    }

    #[test]
    fn content_stream_read_returns_expected_operators_and_assigns_ids() {
        let reader = pdf_object_reader::ObjectReader::new(&PassthroughResolver);
        let parsed = reader
            .read::<ContentStream>(&ObjectVariant::Stream(stream_object(
                1,
                b"BX EX 10 20 m 30 40 l",
            )))
            .expect("stream should parse");

        assert_eq!(parsed.id, 0);
        assert!(matches!(
            parsed.operators.first(),
            Some(PdfOperatorVariant::BeginCompatibility(_))
        ));
        assert!(matches!(
            parsed.operators.get(1),
            Some(PdfOperatorVariant::EndCompatibility(_))
        ));
        assert_eq!(
            recorded_operations(&parsed.operators),
            vec![
                RecordedOperation::MoveTo { x: 10.0, y: 20.0 },
                RecordedOperation::LineTo { x: 30.0, y: 40.0 },
            ]
        );
    }

    #[test]
    fn content_stream_read_handles_bare_sign_text_array_adjustment() {
        let reader = pdf_object_reader::ObjectReader::new(&PassthroughResolver);
        let parsed = reader.read::<ContentStream>(
            &ObjectVariant::Stream(stream_object(
                1,
                b"BT\n/F1 11.67 Tf\n1 0 0 1 10 20 Tm\n[(e)-4(x)12(t)-3(e)-4(n)-4(s)3(i)3(v)-(e)-4(l)3(y)]TJ\nET\n",
            )),
        )
        .expect("stream should parse");

        assert!(matches!(
            parsed.operators.get(3),
            Some(PdfOperatorVariant::ShowTextArray(op))
                if op
                    == &ShowTextArray::new(vec![
                        PdfTextItem::Text(Arc::from(&b"e"[..])),
                        PdfTextItem::Adjustment(-4.0),
                        PdfTextItem::Text(Arc::from(&b"x"[..])),
                        PdfTextItem::Adjustment(12.0),
                        PdfTextItem::Text(Arc::from(&b"t"[..])),
                        PdfTextItem::Adjustment(-3.0),
                        PdfTextItem::Text(Arc::from(&b"e"[..])),
                        PdfTextItem::Adjustment(-4.0),
                        PdfTextItem::Text(Arc::from(&b"n"[..])),
                        PdfTextItem::Adjustment(-4.0),
                        PdfTextItem::Text(Arc::from(&b"s"[..])),
                        PdfTextItem::Adjustment(3.0),
                        PdfTextItem::Text(Arc::from(&b"i"[..])),
                        PdfTextItem::Adjustment(3.0),
                        PdfTextItem::Text(Arc::from(&b"v"[..])),
                        PdfTextItem::Adjustment(0.0),
                        PdfTextItem::Text(Arc::from(&b"e"[..])),
                        PdfTextItem::Adjustment(-4.0),
                        PdfTextItem::Text(Arc::from(&b"l"[..])),
                        PdfTextItem::Adjustment(3.0),
                        PdfTextItem::Text(Arc::from(&b"y"[..])),
                    ])
        ));
    }

    #[test]
    fn parsed_inline_image_can_be_dispatched() {
        let reader = pdf_object_reader::ObjectReader::new(&PassthroughResolver);
        let parsed = reader
            .read::<ContentStream>(&ObjectVariant::Stream(stream_object(
                1,
                b"BI /W 1 /H 1 /BPC 8 /CS /G ID \x00 EI",
            )))
            .expect("inline image should parse");
        let inline_image = parsed
            .operators
            .first()
            .and_then(|operator| match operator {
                PdfOperatorVariant::InlineImage(image) => Some(image),
                _ => None,
            })
            .expect("expected inline image");

        let mut backend = RecordingBackend::default();
        parsed
            .operators
            .first()
            .expect("inline image operator")
            .call(&mut backend)
            .expect("dispatch should succeed");

        assert_eq!(
            backend.operations,
            vec![RecordedOperation::PaintInlineImage {
                data: inline_image.shared_data(),
            }]
        );
    }

    #[test]
    fn content_stream_read_skips_unknown_operator_and_recovers() {
        let reader = pdf_object_reader::ObjectReader::new(&PassthroughResolver);
        let parsed = reader
            .read::<ContentStream>(&ObjectVariant::Stream(stream_object(1, b"@ q")))
            .expect("stream should parse");

        assert_eq!(
            recorded_operations(&parsed.operators),
            vec![RecordedOperation::SaveGraphicsState]
        );
    }

    #[test]
    fn content_stream_read_parses_array_streams_in_order_without_concatenation() {
        let contents = ObjectVariant::Array(
            vec![
                ObjectVariant::Reference(pdf_object_reader::object_id::ObjectId::new(1, 0).into()),
                ObjectVariant::Reference(pdf_object_reader::object_id::ObjectId::new(2, 0)),
            ]
            .into(),
        );
        let resolver = MapResolver {
            objects: BTreeMap::from([
                (1, ObjectVariant::Stream(stream_object(1, b"1 2"))),
                (2, ObjectVariant::Stream(stream_object(2, b"3 4 m"))),
            ]),
        };
        let reader = pdf_object_reader::ObjectReader::new(&resolver);

        let parsed = reader
            .read::<ContentStream>(&contents)
            .expect("contents array should parse");

        assert_eq!(parsed.id, 0);
        assert_eq!(
            recorded_operations(&parsed.operators),
            vec![RecordedOperation::MoveTo { x: 3.0, y: 4.0 }]
        );
    }

    #[test]
    fn content_stream_read_rejects_non_stream_array_entries() {
        let contents = ObjectVariant::Array(vec![ObjectVariant::Null].into());
        let reader = pdf_object_reader::ObjectReader::new(&PassthroughResolver);

        let err = reader
            .read::<ContentStream>(&contents)
            .err()
            .expect("non-stream array entries should fail");

        assert!(matches!(
            err,
            pdf_object_reader::ObjectReadError::At {
                location: pdf_object_reader::ReadLocation::ArrayIndex(0),
                source,
            } if matches!(*source, pdf_object_reader::ObjectReadError::TypeMismatch {
                expected: pdf_object_reader::object_kind::ObjectKind::Stream,
                actual: pdf_object_reader::object_kind::ObjectKind::Null,
            })
        .into()));
        assert_eq!(
            reader
                .content_stream_ids()
                .next_id()
                .expect("id should remain unconsumed"),
            0
        );
    }

    #[test]
    fn content_stream_read_skips_malformed_inline_image_and_consumes_an_id() {
        let reader = pdf_object_reader::ObjectReader::new(&PassthroughResolver);
        let parsed = reader
            .read::<ContentStream>(&ObjectVariant::Stream(stream_object(
                1,
                b"BI /W 1 /H 1 ID abc Q",
            )))
            .expect("malformed inline image should be skipped");

        assert_eq!(parsed.id, 0);
        assert_eq!(
            recorded_operations(&parsed.operators),
            vec![RecordedOperation::RestoreGraphicsState]
        );
        assert_eq!(
            reader
                .content_stream_ids()
                .next_id()
                .expect("next id should advance"),
            1
        );
    }

    #[test]
    fn optional_contents_preserves_allocator_for_missing_contents() {
        let page = Dictionary::new(BTreeMap::<Vec<u8>, ObjectVariant>::new());
        let reader = pdf_object_reader::ObjectReader::new(&PassthroughResolver);

        let contents = reader
            .read::<PageContents>(&ObjectVariant::Dictionary(page))
            .expect("missing contents should not error")
            .0;

        assert!(contents.is_none());
        assert_eq!(
            reader
                .content_stream_ids()
                .next_id()
                .expect("id should still start at zero"),
            0
        );
    }

    #[test]
    fn optional_contents_parses_stream_arrays_and_allocates_monotonically() {
        let contents = ObjectVariant::Array(
            vec![
                ObjectVariant::Reference(pdf_object_reader::object_id::ObjectId::new(1, 0).into()),
                ObjectVariant::Reference(pdf_object_reader::object_id::ObjectId::new(2, 0)),
            ]
            .into(),
        );
        let page = Dictionary::new(BTreeMap::from([(Vec::from(b"Contents"), contents)]));
        let resolver = MapResolver {
            objects: BTreeMap::from([
                (1, ObjectVariant::Stream(stream_object(1, b"q"))),
                (2, ObjectVariant::Stream(stream_object(2, b"Q"))),
            ]),
        };
        let reader = pdf_object_reader::ObjectReader::new(&resolver);

        let content_stream = reader
            .read::<PageContents>(&ObjectVariant::Dictionary(page))
            .expect("contents array should parse")
            .0
            .expect("page should have a content stream");

        assert_eq!(content_stream.id, 0);
        assert_eq!(
            recorded_operations(&content_stream.operators),
            vec![
                RecordedOperation::SaveGraphicsState,
                RecordedOperation::RestoreGraphicsState,
            ]
        );

        let next = reader
            .read::<ContentStream>(&ObjectVariant::Stream(stream_object(3, b"q")))
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
