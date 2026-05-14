use std::collections::BTreeMap;

use pdf_content_stream::{ContentStream, ContentStreamIdAllocator};
use pdf_content_stream_operators::{
    TextElement,
    compatibility_operators::{BeginCompatibility, EndCompatibility},
    graphics_state_operators::{RestoreGraphicsState, SaveGraphicsState},
    path_operators::{LineTo, MoveTo},
    recording_pdf_operator_backend::{RecordedOperation, RecordingBackend},
    text_showing_operators::ShowTextArray,
    variants::PdfOperatorVariant,
};
use pdf_object::{
    dictionary::Dictionary, object_resolver::PassthroughResolver, object_variant::ObjectVariant,
    stream::StreamObject,
};

fn stream_object(object_number: usize, data: &[u8]) -> StreamObject {
    StreamObject::new(
        object_number,
        0,
        Box::new(Dictionary::new(BTreeMap::new())),
        data.to_vec(),
    )
}

#[test]
fn parse_returns_expected_operators() {
    let parsed = pdf_content_stream::parse(b"BX EX 10 20 m 30 40 l").expect("stream should parse");

    assert_eq!(
        parsed,
        vec![
            PdfOperatorVariant::BeginCompatibility(BeginCompatibility),
            PdfOperatorVariant::EndCompatibility(EndCompatibility),
            PdfOperatorVariant::MoveTo(MoveTo::new(10.0, 20.0)),
            PdfOperatorVariant::LineTo(LineTo::new(30.0, 40.0)),
        ]
    );
}

#[test]
fn parse_handles_bare_sign_text_array_adjustment() {
    let parsed = pdf_content_stream::parse(
        b"BT\n/F1 11.67 Tf\n1 0 0 1 10 20 Tm\n[(e)-4(x)12(t)-3(e)-4(n)-4(s)3(i)3(v)-(e)-4(l)3(y)]TJ\nET\n",
    )
    .expect("stream should parse");

    assert!(matches!(
        parsed.get(3),
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
    let parsed =
        pdf_content_stream::parse(b"BI /W 1 /H 1 ID \x00 EI").expect("inline image should parse");
    let inline_image = match parsed.first() {
        Some(PdfOperatorVariant::InlineImage(image)) => image.clone(),
        other => panic!("expected inline image, got {other:?}"),
    };

    let mut backend = RecordingBackend::default();
    parsed[0]
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
fn parse_skips_unknown_operator_and_recovers() {
    let parsed = pdf_content_stream::parse(b"@ q").expect("stream should parse");

    assert_eq!(
        parsed,
        vec![PdfOperatorVariant::SaveGraphicsState(SaveGraphicsState)]
    );
}

#[test]
fn parse_content_stream_from_dictionary_preserves_allocator_for_missing_contents() {
    let page = Dictionary::new(BTreeMap::new());
    let mut ids = ContentStreamIdAllocator::new();

    let contents = pdf_content_stream::parse_content_stream_from_dictionary(
        &page,
        &PassthroughResolver,
        &mut ids,
    )
    .expect("missing contents should not error");

    assert!(contents.is_none());
    assert_eq!(ids.next_id().expect("id should still start at zero"), 0);
}

#[test]
fn parse_content_stream_from_dictionary_concatenates_stream_arrays() {
    let contents = ObjectVariant::Array(vec![
        ObjectVariant::Stream(stream_object(1, b"q")),
        ObjectVariant::Stream(stream_object(2, b"Q")),
    ]);
    let page = Dictionary::new(BTreeMap::from([("Contents".to_string(), contents)]));
    let mut ids = ContentStreamIdAllocator::new();

    let content_stream = pdf_content_stream::parse_content_stream_from_dictionary(
        &page,
        &PassthroughResolver,
        &mut ids,
    )
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

    let next =
        pdf_content_stream::parse_content_stream_from_stream(&stream_object(3, b"q"), &mut ids)
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
