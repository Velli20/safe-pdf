#![allow(clippy::arithmetic_side_effects, clippy::expect_used)]

use std::hint::black_box;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use pdf_parser::parser::PdfParser;

const REPETITIONS: usize = 4_096;

fn repeated_input(fragment: &[u8]) -> Vec<u8> {
    let mut input = Vec::with_capacity(fragment.len() * REPETITIONS);
    for _ in 0..REPETITIONS {
        input.extend_from_slice(fragment);
    }
    input
}

fn benchmark_numbers(criterion: &mut Criterion, name: &str, fragment: &[u8], count: usize) {
    let input = repeated_input(fragment);
    let expected_numbers = count * REPETITIONS;
    let mut group = criterion.benchmark_group(name);
    group.throughput(Throughput::Bytes(
        u64::try_from(input.len()).expect("benchmark input length should fit u64"),
    ));

    group.bench_function("parse", |bencher| {
        bencher.iter(|| {
            let mut parser = PdfParser::from(black_box(input.as_slice()));
            for _ in 0..expected_numbers {
                black_box(
                    parser
                        .parse_number()
                        .expect("benchmark input should contain valid numbers"),
                );
            }
        });
    });
    group.finish();
}

fn benchmark_number_parsing(criterion: &mut Criterion) {
    benchmark_numbers(
        criterion,
        "number_parsing/integer",
        b"12345 -67890 +42 0 ",
        4,
    );
    benchmark_numbers(
        criterion,
        "number_parsing/real",
        b"123.456 -0.789 +3.14 .00048828125 42. ",
        5,
    );
    benchmark_numbers(
        criterion,
        "number_parsing/mixed",
        b"1 0 0 1 10 20 -0.25 +3.14159 .5 42. ",
        10,
    );
}

criterion_group!(benches, benchmark_number_parsing);
criterion_main!(benches);
