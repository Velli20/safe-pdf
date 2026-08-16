use std::collections::BTreeMap;

use pdf_content_stream::{ContentStream, ContentStreamIdAllocator};
use pdf_content_stream_operators::{
    TextElement,
    graphics_state_operators::SaveGraphicsState,
    recording_pdf_operator_backend::{RecordedOperation, RecordingBackend},
    text_showing_operators::ShowTextArray,
    variants::PdfOperatorVariant,
};
use pdf_object::{
    dictionary::Dictionary, error::ObjectError, object_resolver::ObjectResolver,
    object_resolver::PassthroughResolver, object_variant::ObjectVariant, stream::StreamObject,
};

fn stream_object(object_number: usize, data: &[u8]) -> StreamObject {
    StreamObject::new(
        object_number,
        0,
        Box::new(Dictionary::new(BTreeMap::new())),
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

impl ObjectResolver for MapResolver {
    fn resolve_object<'a>(
        &'a self,
        obj: &'a ObjectVariant,
    ) -> Result<&'a ObjectVariant, ObjectError> {
        match obj {
            ObjectVariant::Reference(object_number) => {
                self.objects
                    .get(object_number)
                    .ok_or(ObjectError::FailedResolveObjectReference {
                        obj_num: *object_number,
                    })
            }
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
        &ObjectVariant::Stream(stream_object(1, b"BI /W 1 /H 1 /BPC 8 /CS /G ID \x00 EI")),
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
            data: inline_image.shared_data(),
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
        recorded_operations(&parsed.operators),
        vec![RecordedOperation::SaveGraphicsState]
    );
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
            (1, ObjectVariant::Stream(stream_object(1, b"1 2"))),
            (2, ObjectVariant::Stream(stream_object(2, b"3 4 m"))),
        ]),
    };

    let content_stream = ContentStream::from_dictionary(&page, &resolver, &mut ids)
        .expect("contents array should parse")
        .expect("page should have a content stream");

    assert_eq!(content_stream.id, 0);
    assert_eq!(
        recorded_operations(&content_stream.operators),
        vec![RecordedOperation::MoveTo { x: 3.0, y: 4.0 }]
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
fn content_stream_new_rejects_non_stream_array_entries() {
    let contents = ObjectVariant::Array(vec![ObjectVariant::Null]);
    let mut ids = ContentStreamIdAllocator::new();

    let err = match ContentStream::new(&contents, &PassthroughResolver, &mut ids) {
        Ok(_) => panic!("non-stream array entries should fail"),
        Err(err) => err,
    };

    assert!(matches!(
        err,
        pdf_content_stream_operators::error::PdfOperatorError::Object(ObjectError::TypeMismatch(
            "Stream", "Null"
        ))
    ));
    assert_eq!(ids.next_id().expect("id should remain unconsumed"), 0);
}

#[test]
fn content_stream_new_skips_malformed_inline_image_and_consumes_an_id() {
    let mut ids = ContentStreamIdAllocator::new();
    let parsed = ContentStream::new(
        &ObjectVariant::Stream(stream_object(1, b"BI /W 1 /H 1 ID abc Q")),
        &PassthroughResolver,
        &mut ids,
    )
    .expect("malformed inline image should be skipped");

    assert_eq!(parsed.id, 0);
    assert_eq!(
        recorded_operations(&parsed.operators),
        vec![RecordedOperation::RestoreGraphicsState]
    );
    assert_eq!(ids.next_id().expect("next id should advance"), 1);
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
