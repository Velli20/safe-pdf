use criterion::{Criterion, criterion_group, criterion_main};
use pdf_jbig2::bench_support;
use std::hint::black_box;

fn bench_generic_regions(c: &mut Criterion) {
    let mut group = c.benchmark_group("generic_region");
    group.bench_function("optimized_template0", |b| {
        b.iter(|| black_box(bench_support::decode_optimized_template0()))
    });
    group.bench_function("optimized_template2", |b| {
        b.iter(|| black_box(bench_support::decode_optimized_template2()))
    });
    group.bench_function("unoptimized_with_skip", |b| {
        b.iter(|| black_box(bench_support::decode_unoptimized_with_skip()))
    });
    group.bench_function("dispatch", |b| {
        b.iter(|| black_box(bench_support::decode_generic_region_dispatch()))
    });
    group.finish();
}

fn bench_bitmaps(c: &mut Criterion) {
    let mut group = c.benchmark_group("bitmap");
    group.bench_function("compose_aligned", |b| {
        b.iter(|| black_box(bench_support::compose_aligned_bitmap()))
    });
    group.bench_function("extract_aligned_subimages", |b| {
        b.iter(|| black_box(bench_support::extract_aligned_subimages()))
    });
    group.bench_function("invert_tight_output", |b| {
        b.iter(|| black_box(bench_support::invert_tight_output()))
    });
    group.finish();
}

fn bench_huffman(c: &mut Criterion) {
    c.bench_function("huffman/standard_table", |b| {
        b.iter(|| black_box(bench_support::decode_standard_huffman()))
    });
}

criterion_group!(benches, bench_generic_regions, bench_bitmaps, bench_huffman);
criterion_main!(benches);
