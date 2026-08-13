#![allow(clippy::arithmetic_side_effects, clippy::expect_used)]

use std::{collections::HashMap, hint::black_box, rc::Rc};

use criterion::{Criterion, criterion_group, criterion_main};
use pdf_canvas::{pdf_canvas::PdfCanvas, recording_canvas::RecordingCanvas};
use pdf_content_stream::{ContentStream, ContentStreamIdAllocator};
use pdf_document::page::PdfPage;
use pdf_font::{
    encoding::Encoding,
    font::Font,
    type1_font::{Type1Font, Type1FontProgramFormat},
};
use pdf_object::{
    dictionary::Dictionary, object_resolver::PassthroughResolver, object_variant::ObjectVariant,
    stream::StreamObject,
};
use pdf_resources::{resource::Resource, resources::Resources};

const EEXEC_SEED: u16 = 55665;

fn encrypt(bytes: &[u8]) -> Vec<u8> {
    let mut state = EEXEC_SEED;
    bytes
        .iter()
        .map(|plain| {
            let cipher = *plain ^ u8::try_from(state >> 8).expect("shifted seed should fit");
            state = u16::try_from(
                (u32::from(cipher) + u32::from(state))
                    .wrapping_mul(52845)
                    .wrapping_add(22719)
                    & 0xffff,
            )
            .expect("masked state should fit");
            cipher
        })
        .collect()
}

fn font_bytes() -> Vec<u8> {
    let mut bytes = br#"%!FontType1-1.0: Bench 1.0
10 dict begin
/FontName /Bench def
/FontType 1 def
/FontMatrix [0.001 0 0 0.001 0 0] readonly def
/FontBBox [0 0 0 0] readonly def
/Encoding StandardEncoding def
currentdict end
currentfile eexec
"#
    .to_vec();
    let mut private = vec![0, 0, 0, 0];
    private.extend_from_slice(b"/Private 1 dict dup begin\n/lenIV -1 def\n/CharStrings 1 dict dup begin\n/.notdef 1 RD \x0E ND\nend\nend\nmark currentfile closefile\n");
    bytes.extend(encrypt(&private));
    bytes.extend_from_slice(b"0000000000000000000000000000000000000000\ncleartomark\n");
    bytes
}

fn content_stream(text: &[u8]) -> ContentStream {
    let mut data = b"BT /F1 10 Tf (".to_vec();
    data.extend_from_slice(text);
    data.extend_from_slice(b") Tj ET");
    let stream = StreamObject::new(1, 0, Box::new(Dictionary::new(Default::default())), data);
    ContentStream::new(
        &ObjectVariant::Stream(stream),
        &PassthroughResolver,
        &mut ContentStreamIdAllocator::new(),
    )
    .expect("benchmark stream should parse")
}

fn benchmark(c: &mut Criterion) {
    let font = Font::Type1(Type1Font {
        font_file: font_bytes().into(),
        program_format: Type1FontProgramFormat::ClassicType1,
        widths: Some(HashMap::from([(65, 500.0)])),
        encoding: Encoding::default(),
        to_unicode: None,
    });
    let resources = Resources {
        fonts: HashMap::from([(
            "F1".to_string(),
            Resource::Font {
                font: Rc::new(font),
                resources: None,
            },
        )]),
        ..Default::default()
    };
    let text = vec![b'A'; 1_024];
    let stream = content_stream(&text);
    let page = PdfPage::default();

    c.bench_function("text/render_1024_glyphs", |bencher| {
        bencher.iter(|| {
            let mut recording = RecordingCanvas::new(1000.0, 1000.0);
            let mut canvas =
                PdfCanvas::new(&mut recording, &page, None).expect("canvas should initialize");
            canvas
                .render_content_stream(&stream, None, None, Some(&resources), None)
                .expect("text should render");
        });
    });
    c.bench_function("text/render_and_record_1024_glyphs", |bencher| {
        bencher.iter(|| {
            let mut recording = RecordingCanvas::new(1000.0, 1000.0);
            let mut canvas = PdfCanvas::new(&mut recording, &page, None)
                .expect("canvas should initialize")
                .with_text_recording();
            canvas
                .render_content_stream(&stream, None, None, Some(&resources), None)
                .expect("text should render");
            black_box(canvas.take_text_glyphs());
        });
    });
}

criterion_group!(benches, benchmark);
criterion_main!(benches);
