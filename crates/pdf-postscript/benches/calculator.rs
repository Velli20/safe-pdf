#![allow(
    clippy::arithmetic_side_effects,
    clippy::as_conversions,
    clippy::expect_used
)]

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use pdf_postscript::{calculator::execute, operator::Operator, parser::parse_tokens, value::Value};

const SAMPLES: usize = 256;

/// Tint transform that picks one of four color ramps through nested `ifelse` chains.
const BRANCHING: &str = "{ dup 0.5 lt { dup 0.25 lt { 4 mul 0 0 } { 0.25 sub 4 mul 1 exch 0 } \
    ifelse } { dup 0.75 lt { 0.5 sub 4 mul 1 exch sub 1 0 } { 0.75 sub 4 mul 0 1 3 -1 roll } \
    ifelse } ifelse dup 1 gt { pop 1 } if }";

/// Straight-line arithmetic over two inputs.
const ARITHMETIC: &str = "{ 2 copy mul 3 1 roll dup mul exch dup mul add sqrt 0.5 mul \
    add abs 1 exch sub 0.3 mul 0.2 add }";

/// Stack shuffling over three inputs.
const STACK: &str = "{ 3 copy 2 index 1 index add 4 1 roll mul exch div 5 1 roll \
    3 copy pop pop pop pop 4 2 roll add add exch pop }";

fn parse(code: &str) -> Vec<Operator> {
    let code = code.replace('{', " { ").replace('}', " } ");
    parse_tokens(&code.split_whitespace().collect::<Vec<_>>())
        .expect("benchmark program should parse")
}

fn benchmark_program(criterion: &mut Criterion, name: &str, code: &str, inputs: usize) {
    let ops = parse(code);
    let samples: Vec<Vec<Value>> = (0..SAMPLES)
        .map(|sample| {
            let t = sample as f64 / SAMPLES as f64;
            (0..inputs)
                .map(|input| Value::Real((t + input as f64 * 0.37).fract() + 0.01))
                .collect()
        })
        .collect();

    criterion.bench_function(name, |bencher| {
        bencher.iter(|| {
            for sample in &samples {
                let result = execute(black_box(sample.to_vec()), black_box(&ops))
                    .expect("benchmark program should execute");
                black_box(result);
            }
        });
    });
}

fn calculator_benchmarks(criterion: &mut Criterion) {
    benchmark_program(criterion, "branching", BRANCHING, 1);
    benchmark_program(criterion, "arithmetic", ARITHMETIC, 2);
    benchmark_program(criterion, "stack", STACK, 3);
}

criterion_group!(benches, calculator_benchmarks);
criterion_main!(benches);
