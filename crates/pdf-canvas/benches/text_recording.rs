#![allow(clippy::arithmetic_side_effects, clippy::expect_used)]

use std::{collections::HashMap, hint::black_box, sync::Arc};

use bytes::Bytes;
use criterion::{Criterion, criterion_group, criterion_main};
use pdf_canvas::{pdf_canvas::PdfCanvas, recording_canvas::RecordingCanvas};
use pdf_content_stream::ContentStream;
use pdf_document::page::PdfPage;
use pdf_font::{FontProgramFormat, FontSource, PdfFontSpec};
use pdf_object_reader::{
    EmptySource, dictionary::Dictionary, object_variant::ObjectVariant, pdf_name::PdfName,
    stream::StreamObject,
};
use pdf_resources::{resource::Resource, resources::Resources};
use pdf_text_engine::bundled_font_system;

/// Builds a text object that shows `text` at 10 points.
fn content_stream(text: &[u8]) -> ContentStream {
    let mut data = b"BT /F1 10 Tf (".to_vec();
    data.extend_from_slice(text);
    data.extend_from_slice(b") Tj ET");
    parse(data)
}

/// Builds a text object that shows 16 `A` glyphs at each size from 1 to 64 points.
fn sized_content_stream() -> ContentStream {
    let mut data = b"BT".to_vec();
    for size in 1..=64 {
        data.extend_from_slice(format!(" /F1 {size} Tf (").as_bytes());
        data.extend_from_slice(&[b'A'; 16]);
        data.extend_from_slice(b") Tj");
    }
    data.extend_from_slice(b" ET");
    parse(data)
}

fn parse(data: Vec<u8>) -> ContentStream {
    let stream = StreamObject::new(1, 0, Dictionary::default(), data);
    pdf_object_reader::ObjectReader::new(EmptySource)
        .read::<ContentStream>(&ObjectVariant::Stream(stream))
        .expect("benchmark stream should parse")
}

fn benchmark(c: &mut Criterion) {
    let font = PdfFontSpec::from(pdf_font::Standard14Font::Helvetica);
    let PdfFontSpec::Type1(simple) = font else {
        unreachable!("Helvetica should create a simple Type 1 specification");
    };
    let font = PdfFontSpec::Type1(pdf_font::SimpleFontSpec {
        program: Some(FontSource::Memory {
            data: Bytes::from_static(pdf_font::standard14::fallback_font_bytes(
                pdf_font::Standard14Font::Helvetica,
            )),
            format: FontProgramFormat::Cff,
            face_index: 0,
        }),
        ..simple
    });
    let font_system = bundled_font_system();
    let resources = Arc::new(Resources {
        fonts: HashMap::from([(
            PdfName::from(b"F1"),
            Resource::Font {
                font: Arc::new(font),
                resources: None,
            }
            .into(),
        )]),
        ..Default::default()
    });
    let page = PdfPage::default();
    let repeated = vec![b'A'; 1_024];
    let mixed = (0..1_024)
        .map(|index| b'A' + u8::try_from(index % 26).expect("alphabet index should fit"))
        .collect::<Vec<_>>();

    for (workload, stream) in [
        ("repeated", content_stream(&repeated)),
        ("mixed", content_stream(&mixed)),
        ("repeated_at_64_sizes", sized_content_stream()),
    ] {
        c.bench_function(
            &format!("text/render_{workload}_1024_real_glyphs"),
            |bencher| {
                bencher.iter(|| {
                    let mut recording = RecordingCanvas::new(1000.0, 1000.0);
                    let mut canvas =
                        PdfCanvas::new(&mut recording, &page, None, Arc::clone(&font_system))
                            .expect("canvas should initialize");
                    canvas
                        .render_content_stream(
                            &stream,
                            None,
                            None,
                            Some(Arc::clone(&resources)),
                            None,
                        )
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
                        .render_content_stream(
                            &stream,
                            None,
                            None,
                            Some(Arc::clone(&resources)),
                            None,
                        )
                        .expect("text should render");
                    black_box(canvas.take_text_glyphs());
                });
            },
        );
    }
}

criterion_group!(benches, benchmark);
criterion_main!(benches);
