#![allow(clippy::arithmetic_side_effects, clippy::expect_used)]

use std::hint::black_box;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use pdf_content_stream::{ContentStream, ContentStreamIdAllocator};
use pdf_object::{
    dictionary::Dictionary, object_resolver::PassthroughResolver, object_variant::ObjectVariant,
    stream::StreamObject,
};

const REPETITIONS: usize = 1_024;

fn repeated_stream(fragment: &[u8]) -> ObjectVariant {
    let mut data = Vec::with_capacity(fragment.len() * REPETITIONS);
    for _ in 0..REPETITIONS {
        data.extend_from_slice(fragment);
    }

    ObjectVariant::Stream(StreamObject::new(
        1,
        0,
        Dictionary::new(Default::default()),
        data,
    ))
}

fn benchmark_stream(
    criterion: &mut Criterion,
    name: &str,
    fragment: &[u8],
    operators_per_fragment: usize,
) {
    let stream = repeated_stream(fragment);
    let ObjectVariant::Stream(stream_object) = &stream else {
        return;
    };
    let input_size = stream_object.raw_data().len();
    let expected_operators = operators_per_fragment * REPETITIONS;
    let mut group = criterion.benchmark_group(name);
    group.throughput(Throughput::Bytes(
        u64::try_from(input_size).expect("benchmark input length should fit u64"),
    ));

    group.bench_function("materialize", |bencher| {
        bencher.iter(|| {
            let mut allocator = ContentStreamIdAllocator::new();
            let parsed =
                ContentStream::new(black_box(&stream), &PassthroughResolver, &mut allocator)
                    .expect("benchmark content stream should parse");
            assert_eq!(parsed.operators.len(), expected_operators);
            black_box(parsed);
        });
    });
    group.finish();
}

fn benchmark_operator_parsing(criterion: &mut Criterion) {
    benchmark_stream(
        criterion,
        "operator_parsing/numeric_heavy",
        b"q 1 0 0 1 10 20 cm 10 20 m 30 40 l 1 2 3 4 5 6 c 10 20 30 40 re 0.1 0.2 0.3 rg Q\n",
        8,
    );
    benchmark_stream(
        criterion,
        "operator_parsing/dispatch_heavy",
        b"q Q BT ET BX EX h n S s f f* W W*\n",
        14,
    );
    benchmark_stream(
        criterion,
        "operator_parsing/mixed",
        b"q /F1 12 Tf BT 1 0 0 1 10 20 Tm (Hello) Tj [(A) -20 <42> 5] TJ ET /DeviceRGB cs 0.1 0.2 0.3 sc 0.1 0.2 0.3 /P1 scn /X1 Do Q\n",
        12,
    );
}

criterion_group!(benches, benchmark_operator_parsing);
criterion_main!(benches);
