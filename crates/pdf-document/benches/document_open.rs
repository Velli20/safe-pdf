#![allow(clippy::expect_used)]

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use pdf_document::reader::PdfReader;

/// Example documents read from parsing the header through decoding every page.
const DOCUMENTS: &[&str] = &[
    "Gradients.pdf",
    "PlaygroundMDN.pdf",
    "RadialGradientFills.pdf",
    "W3Schools.pdf",
    "dd5cf1a7d6d190f94a28201777f11bf4.pdf",
    "is.pdf",
    "test6.pdf",
    "webgl.pdf",
];

fn document_open(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("document_open");
    group.sample_size(10);
    for name in DOCUMENTS {
        let path = format!(
            "{}/../../examples/assets/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        let input = std::fs::read(&path).expect("example document should be readable");
        group.bench_function(*name, |bencher| {
            bencher.iter(|| {
                let document = PdfReader
                    .read_from_bytes(black_box(&input), None)
                    .expect("example document should open");
                black_box(document.page_count())
            });
        });
    }
    group.finish();
}

criterion_group!(benches, document_open);
criterion_main!(benches);
