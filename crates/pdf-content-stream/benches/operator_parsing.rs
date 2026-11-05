#![allow(clippy::arithmetic_side_effects, clippy::expect_used)]

use std::{convert::Infallible, hint::black_box};

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use pdf_content_stream::ContentStream;
use pdf_object_reader::{
    EmptySource, ObjectReader, ObjectSource, dictionary::Dictionary, object_id::ObjectId,
    object_variant::ObjectVariant, pdf_object::PdfObject, stream::StreamObject,
};

const REPETITIONS: usize = 1_024;
const STREAMS_PER_ARRAY: usize = 8;

/// Serves the entries of a `/Contents` array, numbered from one.
struct StreamArraySource(Vec<ObjectVariant>);

impl ObjectSource for StreamArraySource {
    type Error = Infallible;

    fn read_object(&self, id: ObjectId) -> Result<Option<PdfObject>, Self::Error> {
        Ok(id
            .number
            .checked_sub(1)
            .and_then(|index| self.0.get(index))
            .cloned()
            .map(PdfObject::new))
    }
}

fn repeated_stream(fragment: &[u8]) -> ObjectVariant {
    let mut data = Vec::with_capacity(fragment.len() * REPETITIONS);
    for _ in 0..REPETITIONS {
        data.extend_from_slice(fragment);
    }

    ObjectVariant::Stream(StreamObject::new(1, 0, Dictionary::default(), data))
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
            let reader = pdf_object_reader::ObjectReader::new(EmptySource);
            let parsed = reader
                .read::<ContentStream>(black_box(&stream))
                .expect("benchmark content stream should parse");
            assert_eq!(parsed.operators.len(), expected_operators);
            black_box(parsed);
        });
    });
    group.finish();
}

/// Measures a content stream divided evenly across the streams of a `/Contents` array.
fn benchmark_stream_array(
    criterion: &mut Criterion,
    name: &str,
    fragment: &[u8],
    operators_per_fragment: usize,
) {
    let repetitions = REPETITIONS / STREAMS_PER_ARRAY;
    let stream_data = fragment.repeat(repetitions);
    let source = StreamArraySource(
        (1..=STREAMS_PER_ARRAY)
            .map(|number| {
                ObjectVariant::Stream(StreamObject::new(
                    number,
                    0,
                    Dictionary::default(),
                    stream_data.clone(),
                ))
            })
            .collect(),
    );
    let contents = ObjectVariant::Array(
        (1..=STREAMS_PER_ARRAY)
            .map(|number| ObjectVariant::Reference(ObjectId::new(number, 0)))
            .collect::<Vec<_>>()
            .into(),
    );
    let input_size = stream_data.len() * STREAMS_PER_ARRAY;
    let expected_operators = operators_per_fragment * repetitions * STREAMS_PER_ARRAY;
    let mut group = criterion.benchmark_group(name);
    group.throughput(Throughput::Bytes(
        u64::try_from(input_size).expect("benchmark input length should fit u64"),
    ));

    group.bench_function("materialize", |bencher| {
        bencher.iter(|| {
            let reader = ObjectReader::new(&source);
            let parsed = reader
                .read::<ContentStream>(black_box(&contents))
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
    let mixed: &[u8] = b"q /F1 12 Tf BT 1 0 0 1 10 20 Tm (Hello) Tj [(A) -20 <42> 5] TJ ET /DeviceRGB cs 0.1 0.2 0.3 sc 0.1 0.2 0.3 /P1 scn /X1 Do Q\n";
    benchmark_stream(criterion, "operator_parsing/mixed", mixed, 12);
    benchmark_stream_array(criterion, "operator_parsing/mixed_stream_array", mixed, 12);
    benchmark_stream(
        criterion,
        "operator_parsing/name_and_text_heavy",
        b"BT /F1 12 Tf [(A) -20 (bc) 15 (d) -5 (efg)] TJ (Hello) Tj ET /GS0 gs /Im0 Do /Span << /MCID 0 >> BDC EMC\n",
        9,
    );
}

criterion_group!(benches, benchmark_operator_parsing);
criterion_main!(benches);
