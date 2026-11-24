#![allow(clippy::arithmetic_side_effects, clippy::expect_used)]

use std::{collections::HashMap, fmt::Write, sync::Arc};

use criterion::{Criterion, criterion_group, criterion_main};
use pdf_canvas::{pdf_canvas::PdfCanvas, recording_canvas::RecordingCanvas};
use pdf_content_stream::ContentStream;
use pdf_document::page::PdfPage;
use pdf_graphics::rect::Rect;
use pdf_object_reader::{
    EmptySource, ObjectHandle, dictionary::Dictionary, object_variant::ObjectVariant,
    pdf_name::PdfName, stream::StreamObject,
};
use pdf_resources::{
    pattern::{PaintType, Pattern, TilingType},
    resource::Resource,
    resources::Resources,
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

/// Appends a path of `segments` line segments, ended by `operator`.
fn push_polyline(data: &mut String, y: usize, segments: usize, operator: &str) {
    write!(data, "0 {y} m").expect("writing to a string should succeed");
    for x in 1..=segments {
        write!(data, " {x} {} l", y + x % 7).expect("writing to a string should succeed");
    }
    writeln!(data, " {operator}").expect("writing to a string should succeed");
}

/// Builds a stream that fills, then strokes, `count` paths of 100 line segments each.
fn long_path_stream(count: usize) -> ContentStream {
    let mut data = String::new();
    for index in 0..count {
        push_polyline(&mut data, index % 1000, 100, "f");
        push_polyline(&mut data, index % 1000, 100, "S");
    }
    parse(data)
}

/// Builds a stream that paints `count` paths of 20 line segments with one fill-and-stroke.
fn fill_and_stroke_stream(count: usize) -> ContentStream {
    let mut data = String::new();
    for index in 0..count {
        push_polyline(&mut data, index % 1000, 20, "B");
    }
    parse(data)
}

/// Builds a stream that strokes `count` pairs of a 1,000-segment and a one-segment path.
fn alternating_path_stream(count: usize) -> ContentStream {
    let mut data = String::new();
    for index in 0..count {
        push_polyline(&mut data, index % 1000, 1_000, "S");
        push_polyline(&mut data, index % 1000, 1, "S");
    }
    parse(data)
}

/// Builds a stream that selects the tiling pattern `P0`, then fills `count` rectangles with it.
fn tiling_fill_stream(count: usize) -> ContentStream {
    let mut data = String::from("/Pattern cs /P0 scn\n");
    for index in 0..count {
        let x = index % 100;
        let y = index / 100;
        writeln!(data, "{x} {y} 4 4 re f").expect("writing to a string should succeed");
    }
    parse(data)
}

/// Resources holding a colored tiling pattern `P0` whose cell draws two squares.
fn tiling_resources() -> Arc<Resources> {
    let pattern = Pattern::Tiling {
        paint_type: PaintType::Colored,
        tiling_type: TilingType::ConstantSpacing,
        bbox: Rect::new(4.0, 4.0),
        x_step: 4.0,
        y_step: 4.0,
        matrix: None,
        resources: ObjectHandle::from(Resources::default()),
        content_stream: parse(String::from("1 0 0 rg 0 0 2 2 re f 2 2 2 2 re f")),
    };
    Arc::new(Resources {
        patterns: HashMap::from([(
            PdfName::from(b"P0"),
            Resource::Pattern(ObjectHandle::from(pattern)),
        )]),
        ..Default::default()
    })
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
    for (name, stream, resources) in [
        (
            "dispatch/render_5000_colored_paths",
            path_stream(5_000),
            None,
        ),
        (
            "dispatch/save_restore_5000_with_font_and_dash",
            save_restore_stream(5_000),
            None,
        ),
        (
            "dispatch/dashed_strokes_5000",
            dashed_stroke_stream(5_000),
            None,
        ),
        ("dispatch/long_paths_1000", long_path_stream(1_000), None),
        (
            "dispatch/fill_and_stroke_2000",
            fill_and_stroke_stream(2_000),
            None,
        ),
        (
            "dispatch/alternating_paths_200",
            alternating_path_stream(200),
            None,
        ),
        (
            "dispatch/tiling_fills_5000",
            tiling_fill_stream(5_000),
            Some(tiling_resources()),
        ),
    ] {
        c.bench_function(name, |bencher| {
            bencher.iter(|| {
                let mut recording = RecordingCanvas::new(1000.0, 1000.0);
                let mut canvas =
                    PdfCanvas::new(&mut recording, &page, None, Arc::clone(&font_system))
                        .expect("canvas should initialize");
                canvas
                    .render_content_stream(&stream, None, None, resources.clone(), None)
                    .expect("stream should render");
            });
        });
    }
}

criterion_group!(benches, benchmark);
criterion_main!(benches);
