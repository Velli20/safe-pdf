#![allow(clippy::arithmetic_side_effects, clippy::expect_used)]

use std::{fmt::Write, sync::Arc};

use criterion::{Criterion, criterion_group, criterion_main};
use pdf_canvas::{pdf_canvas::PdfCanvas, recording_canvas::RecordingCanvas};
use pdf_content_stream::ContentStream;
use pdf_document::page::PdfPage;
use pdf_object_reader::{
    EmptySource, dictionary::Dictionary, object_variant::ObjectVariant, stream::StreamObject,
};
use pdf_text_engine::bundled_font_system;

/// Builds a stream of `count` saved, colored, filled and stroked paths.
fn path_stream(count: usize) -> ContentStream {
    let mut data = String::new();
    for index in 0..count {
        let x = index % 100;
        let y = index / 100;
        writeln!(
            data,
            "q 0.5 g 1 0 0 RG {x} {y} m {x} 5 l 3 4 5 6 {y} 2 c h f \
             {x} {y} 4 4 re 0 0 1 rg f 0.2 0.3 0.4 0.5 K {x} {y} m 9 {y} l S Q"
        )
        .expect("writing to a string should succeed");
    }
    parse(data)
}

/// Builds a stream that selects a font and a dash, then saves and restores `count` times.
fn save_restore_stream(count: usize) -> ContentStream {
    let mut data = String::from("BT /F1 12 Tf ET [3 1] 0 d\n");
    for index in 0..count {
        let x = index % 100;
        let y = index / 100;
        writeln!(data, "q 1 0 0 1 {x} {y} cm 0 0 1 1 re f Q")
            .expect("writing to a string should succeed");
    }
    parse(data)
}

/// Builds a stream that selects a dash, then strokes `count` lines with it.
fn dashed_stroke_stream(count: usize) -> ContentStream {
    let mut data = String::from("[3 1] 0 d\n");
    for index in 0..count {
        let x = index % 100;
        let y = index / 100;
        writeln!(data, "{x} {y} m 9 {y} l S").expect("writing to a string should succeed");
    }
    parse(data)
}

fn parse(data: String) -> ContentStream {
    let stream = StreamObject::new(1, 0, Dictionary::default(), data.into_bytes());
    pdf_object_reader::ObjectReader::new(EmptySource)
        .read::<ContentStream>(&ObjectVariant::Stream(stream))
        .expect("benchmark stream should parse")
}

fn benchmark(c: &mut Criterion) {
    let font_system = bundled_font_system();
    let page = PdfPage::default();
    for (name, stream) in [
        ("dispatch/render_5000_colored_paths", path_stream(5_000)),
        (
            "dispatch/save_restore_5000_with_font_and_dash",
            save_restore_stream(5_000),
        ),
        ("dispatch/dashed_strokes_5000", dashed_stroke_stream(5_000)),
    ] {
        c.bench_function(name, |bencher| {
            bencher.iter(|| {
                let mut recording = RecordingCanvas::new(1000.0, 1000.0);
                let mut canvas =
                    PdfCanvas::new(&mut recording, &page, None, Arc::clone(&font_system))
                        .expect("canvas should initialize");
                canvas
                    .render_content_stream(&stream, None, None, None, None)
                    .expect("stream should render");
            });
        });
    }
}

criterion_group!(benches, benchmark);
criterion_main!(benches);
