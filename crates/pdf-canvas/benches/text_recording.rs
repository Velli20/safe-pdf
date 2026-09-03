#![allow(clippy::arithmetic_side_effects, clippy::expect_used)]

use std::{collections::HashMap, hint::black_box, rc::Rc, sync::Arc};

use bytes::Bytes;
use criterion::{Criterion, criterion_group, criterion_main};
use pdf_canvas::{pdf_canvas::PdfCanvas, recording_canvas::RecordingCanvas};
use pdf_content_stream::{ContentStream, ContentStreamIdAllocator};
use pdf_document::page::PdfPage;
use pdf_font::{FontProgramFormat, FontSource, PdfFontSpec};
use pdf_object::{
    dictionary::Dictionary, object_resolver::PassthroughResolver, object_variant::ObjectVariant,
    stream::StreamObject,
};
use pdf_resources::{resource::Resource, resources::Resources};
use pdf_text_engine::bundled_font_system;

fn content_stream(text: &[u8]) -> ContentStream {
    let mut data = b"BT /F1 10 Tf (".to_vec();
    data.extend_from_slice(text);
    data.extend_from_slice(b") Tj ET");
    let stream = StreamObject::new(1, 0, Dictionary::new(Default::default()), data);
    ContentStream::new(
        &ObjectVariant::Stream(stream),
        &PassthroughResolver,
        &mut ContentStreamIdAllocator::new(),
    )
    .expect("benchmark stream should parse")
}

fn benchmark(c: &mut Criterion) {
    let font = PdfFontSpec::from(pdf_font::Standard14Font::Helvetica);
    let PdfFontSpec::Type1(simple) = font else {
        unreachable!("Helvetica should create a simple Type 1 specification");
    };
    let font = PdfFontSpec::TrueType(pdf_font::SimpleFontSpec {
        program: Some(FontSource::Memory {
            data: Bytes::from_static(pdf_font::standard14::fallback_font_bytes(
                pdf_font::Standard14Font::Helvetica,
            )),
            format: FontProgramFormat::TrueType,
            face_index: 0,
        }),
        ..simple
    });
    let font_system = bundled_font_system();
    let resources = Resources {
        fonts: HashMap::from([(
            b"F1".to_vec(),
            Resource::Font {
                font: Rc::new(font),
                resources: None,
            },
        )]),
        ..Default::default()
    };
    let page = PdfPage::default();
    let repeated = vec![b'A'; 1_024];
    let mixed = (0..1_024)
        .map(|index| b'A' + u8::try_from(index % 26).expect("alphabet index should fit"))
        .collect::<Vec<_>>();

    for (workload, text) in [("repeated", repeated), ("mixed", mixed)] {
        let stream = content_stream(&text);
        c.bench_function(
            &format!("text/render_{workload}_1024_real_glyphs"),
            |bencher| {
                bencher.iter(|| {
                    let mut recording = RecordingCanvas::new(1000.0, 1000.0);
                    let mut canvas =
                        PdfCanvas::new(&mut recording, &page, None, Arc::clone(&font_system))
                            .expect("canvas should initialize");
                    canvas
                        .render_content_stream(&stream, None, None, Some(&resources), None)
                        .expect("text should render");
                });
            },
        );
        c.bench_function(
            &format!("text/render_and_record_{workload}_1024_real_glyphs"),
            |bencher| {
                bencher.iter(|| {
                    let mut recording = RecordingCanvas::new(1000.0, 1000.0);
                    let mut canvas =
                        PdfCanvas::new(&mut recording, &page, None, Arc::clone(&font_system))
                            .expect("canvas should initialize")
                            .with_text_recording();
                    canvas
                        .render_content_stream(&stream, None, None, Some(&resources), None)
                        .expect("text should render");
                    black_box(canvas.take_text_glyphs());
                });
            },
        );
    }
}

criterion_group!(benches, benchmark);
criterion_main!(benches);
