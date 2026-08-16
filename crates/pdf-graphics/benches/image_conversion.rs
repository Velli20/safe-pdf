#![allow(clippy::arithmetic_side_effects)]

use std::{hint::black_box, sync::Arc};

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use pdf_graphics::{Image, PixelFormat};

const WIDTH: usize = 512;
const HEIGHT: usize = 512;
const NUM_PIXELS: usize = WIDTH * HEIGHT;
const NUM_PIXELS_U64: u64 = 262_144;

fn benchmark_image_conversion(criterion: &mut Criterion) {
    let gray = Arc::new(vec![0x80; NUM_PIXELS]);
    let rgb = Arc::new([0x20, 0x80, 0xE0].repeat(NUM_PIXELS));
    let cmyk = Arc::new([0x10, 0x40, 0x80, 0x20].repeat(NUM_PIXELS));
    let two_components = Arc::new([0x40, 0xC0].repeat(NUM_PIXELS));
    let soft_mask = Image {
        data: Arc::new(vec![0xA0; NUM_PIXELS]),
        width: WIDTH,
        height: HEIGHT,
        pixel_format: PixelFormat::Gray8,
    };

    let mut group = criterion.benchmark_group("image_conversion");
    group.throughput(Throughput::Elements(NUM_PIXELS_U64));

    group.bench_function("gray_with_soft_mask", |bencher| {
        bencher.iter(|| {
            black_box(Image::from_decoded_samples(
                Arc::clone(&gray),
                WIDTH,
                HEIGHT,
                1,
                Some(&soft_mask),
            ))
        });
    });
    group.bench_function("rgb", |bencher| {
        bencher.iter(|| {
            black_box(Image::from_decoded_samples(
                Arc::clone(&rgb),
                WIDTH,
                HEIGHT,
                3,
                None,
            ))
        });
    });
    group.bench_function("cmyk", |bencher| {
        bencher.iter(|| {
            black_box(Image::from_decoded_samples(
                Arc::clone(&cmyk),
                WIDTH,
                HEIGHT,
                4,
                None,
            ))
        });
    });
    group.bench_function("two_component_fallback", |bencher| {
        bencher.iter(|| {
            black_box(Image::from_decoded_samples(
                Arc::clone(&two_components),
                WIDTH,
                HEIGHT,
                2,
                None,
            ))
        });
    });

    group.finish();
}

criterion_group!(benches, benchmark_image_conversion);
criterion_main!(benches);
