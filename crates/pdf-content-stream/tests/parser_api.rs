use std::collections::BTreeMap;
use std::sync::Arc;

use pdf_content_stream::ContentStream;
use pdf_content_stream_operators::{
    PdfTextItem,
    variants::PdfOperatorVariant::{self, *},
};
use pdf_object_reader::{
    EmptySource, dictionary::Dictionary, object_variant::ObjectVariant, stream::StreamObject,
};

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

#[test]
fn content_stream_read_returns_expected_operators_and_assigns_ids() {
    let reader = pdf_object_reader::ObjectReader::new(EmptySource);
    let parsed = reader
        .read::<ContentStream>(&ObjectVariant::Stream(stream_object(
            1,
            b"BX EX 10 20 m 30 40 l",
        )))
        .expect("stream should parse");

    assert_eq!(parsed.id, 0);
    assert!(matches!(
        parsed.operators.as_slice(),
        [
            BeginCompatibility,
            EndCompatibility,
            MoveTo { x: 10.0, y: 20.0 },
            LineTo { x: 30.0, y: 40.0 }
        ]
    ));
}

#[test]
fn content_stream_read_handles_bare_sign_text_array_adjustment() {
    let reader = pdf_object_reader::ObjectReader::new(EmptySource);
    let parsed = reader.read::<ContentStream>(
        &ObjectVariant::Stream(stream_object(
            1,
            b"BT\n/F1 11.67 Tf\n1 0 0 1 10 20 Tm\n[(e)-4(x)12(t)-3(e)-4(n)-4(s)3(i)3(v)-(e)-4(l)3(y)]TJ\nET\n",
        )),
    )
    .expect("stream should parse");

    assert!(matches!(
        parsed.operators.get(3),
        Some(PdfOperatorVariant::ShowTextArray { elements })
            if elements
                == &vec![
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
                ]
    ));
}

#[test]
fn content_stream_preserves_non_utf8_font_name() {
    let reader = pdf_object_reader::ObjectReader::new(EmptySource);
    let parsed = reader
        .read::<ContentStream>(&ObjectVariant::Stream(stream_object(1, b"/#FF 12 Tf")))
        .expect("non-UTF-8 resource names should parse");

    assert!(
        matches!(parsed.operators.as_slice(), [SetFont { name, size: 12.0 }] if **name == [0xFF])
    );
}

#[test]
fn content_stream_read_parses_inline_image() {
    let reader = pdf_object_reader::ObjectReader::new(EmptySource);
    let parsed = reader
        .read::<ContentStream>(&ObjectVariant::Stream(stream_object(
            1,
            b"BI /W 1 /H 1 /BPC 8 /CS /G ID \x00 EI",
        )))
        .expect("inline image should parse");

    assert!(matches!(parsed.operators.as_slice(), [InlineImage(_)]));
}

#[test]
fn content_stream_read_skips_unknown_operator_and_recovers() {
    let reader = pdf_object_reader::ObjectReader::new(EmptySource);
    let parsed = reader
        .read::<ContentStream>(&ObjectVariant::Stream(stream_object(1, b"@ q")))
        .expect("stream should parse");

    assert!(matches!(parsed.operators.as_slice(), [SaveGraphicsState]));
}

#[test]
fn optional_contents_preserves_allocator_for_missing_contents() {
    let page = Dictionary::new(BTreeMap::<Vec<u8>, ObjectVariant>::new());
    let reader = pdf_object_reader::ObjectReader::new(EmptySource);

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
            ObjectVariant::Reference(pdf_object_reader::object_id::ObjectId::new(1, 0)),
            ObjectVariant::Reference(pdf_object_reader::object_id::ObjectId::new(2, 0)),
        ]
        .into(),
    );
    let page = Dictionary::new(BTreeMap::from([(Vec::from(b"Contents"), contents)]));
    let resolver = MapResolver {
        objects: BTreeMap::from([
            (1, ObjectVariant::Stream(stream_object(1, b"1 2"))),
            (2, ObjectVariant::Stream(stream_object(2, b"m"))),
        ]),
    };
    let reader = pdf_object_reader::ObjectReader::new(&resolver);

    let content_stream = reader
        .read::<PageContents>(&ObjectVariant::Dictionary(page))
        .expect("contents array should parse")
        .0
        .expect("page should have a content stream");

    assert_eq!(content_stream.id, 0);
    assert!(matches!(
        content_stream.operators.as_slice(),
        [MoveTo { x: 1.0, y: 2.0 }]
    ));

    let next = reader
        .read::<ContentStream>(&ObjectVariant::Stream(stream_object(3, b"q")))
        .expect("follow-up stream should parse");
    assert_eq!(next.id, 1);
}

#[test]
fn content_stream_read_rejects_non_stream_array_entries() {
    let contents = ObjectVariant::Array(vec![ObjectVariant::Integer(0)].into());
    let reader = pdf_object_reader::ObjectReader::new(EmptySource);

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
            actual: pdf_object_reader::object_kind::ObjectKind::Integer,
        })));
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
    let reader = pdf_object_reader::ObjectReader::new(EmptySource);
    let parsed = reader
        .read::<ContentStream>(&ObjectVariant::Stream(stream_object(
            1,
            b"BI /W 1 /H 1 ID abc Q",
        )))
        .expect("malformed inline image should be skipped");

    assert_eq!(parsed.id, 0);
    assert!(matches!(
        parsed.operators.as_slice(),
        [RestoreGraphicsState]
    ));
    assert_eq!(
        reader
            .content_stream_ids()
            .next_id()
            .expect("next id should advance"),
        1
    );
}

#[test]
fn content_stream_is_plain_data_after_parsing() {
    let content_stream = ContentStream {
        operators: vec![PdfOperatorVariant::SaveGraphicsState],
        id: 9,
    };

    assert_eq!(content_stream.id, 9);
    assert_eq!(content_stream.operators.len(), 1);
}
