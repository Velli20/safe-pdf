#![allow(
    clippy::expect_used,
    clippy::arithmetic_side_effects,
    clippy::as_conversions
)]

use std::{collections::BTreeMap, hint::black_box, io::Write};

use bytes::Bytes;
use criterion::{Criterion, criterion_group, criterion_main};
use flate2::{Compression, write::ZlibEncoder};
use pdf_object_reader::{
    dictionary::Dictionary, pdf_string::PdfString, stream::StreamObject, string_kind::StringKind,
};

/// Repeated content stream operators: text that compresses at a high ratio.
fn content_samples() -> Vec<u8> {
    let operators = b"BT /F1 12 Tf 72 712 Td (Hello, world) Tj ET 0.5 0 0 RG 10 10 200 100 re S q 1 0 0 1 50 50 cm Q\n";
    operators.iter().copied().cycle().take(2 << 20).collect()
}

/// RGB gradient samples, like an uncompressed image.
fn image_samples() -> Vec<u8> {
    let (width, height) = (1024_usize, 1024_usize);
    (0..height)
        .flat_map(|y| (0..width).flat_map(move |x| [x as u8, y as u8, (x + y) as u8]))
        .collect()
}

/// Pseudo-random bytes from a fixed LCG: a low-ratio stream, like an embedded font.
fn font_samples() -> Vec<u8> {
    let mut state = 0x2545_f491_u32;
    (0..512 << 10)
        .map(|index| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            // Mix in a repeating pattern so the stream is not incompressible.
            if index % 4 == 0 {
                (index / 4) as u8
            } else {
                (state >> 24) as u8
            }
        })
        .collect()
}

/// Wraps `samples` compressed with zlib into a `/FlateDecode` stream.
fn flate_stream(samples: &[u8]) -> StreamObject {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(samples)
        .expect("zlib write should succeed");
    let compressed = encoder.finish().expect("zlib finish should succeed");
    let dictionary = Dictionary::from_iter(BTreeMap::from([(
        Vec::from(b"Filter"),
        PdfString::from(b"FlateDecode", StringKind::Name),
    )]));
    StreamObject::new_encoded(1, 0, dictionary, Bytes::from(compressed))
}

fn flate_decode(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("flate_decode");
    for (name, samples) in [
        ("content", content_samples()),
        ("image", image_samples()),
        ("font", font_samples()),
    ] {
        let stream = flate_stream(&samples);
        group.bench_function(name, |bencher| {
            bencher.iter(|| {
                let decoded = pdf_filter::filter::decode(black_box(stream.clone()))
                    .expect("stream should decode");
                black_box(decoded.len())
            });
        });
    }
    group.finish();
}

criterion_group!(benches, flate_decode);
criterion_main!(benches);
