#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::as_conversions
)]

use std::collections::BTreeMap;

use pdf_object::{
    cross_reference_table::{CrossReferenceEntry, CrossReferenceEntryType},
    object_resolver::PassthroughResolver,
    object_variant::ObjectVariant,
};
use pdf_parser::parser::PdfParser;

fn format_xref_entry(offset: usize, generation: u16, used: bool) -> String {
    let kind = if used { 'n' } else { 'f' };
    format!("{:010} {:05} {} \n", offset, generation, kind)
}

fn assert_indirect_object_at_offset(
    input: &[u8],
    offset: usize,
    object_number: usize,
    generation_number: usize,
) {
    let mut parser = PdfParser::from(input);
    parser.tokenizer.position = offset;

    let parsed = parser
        .parse_indirect_object(&PassthroughResolver)
        .expect("expected to parse indirect object at recovered offset");

    match parsed {
        Some(ObjectVariant::IndirectObject(indirect_object)) => {
            assert_eq!(indirect_object.object_number, object_number);
            assert_eq!(indirect_object.generation_number, generation_number);
        }
        Some(ObjectVariant::Stream(stream)) => {
            assert_eq!(stream.object_number, object_number);
            assert_eq!(stream.generation_number, generation_number);
        }
        other => panic!("expected indirect object or stream at offset, got {other:?}"),
    }
}

fn build_issue139_like_pdf() -> (Vec<u8>, BTreeMap<usize, usize>) {
    let mut data = Vec::new();
    data.extend_from_slice(b"%PDF-1.4\n");

    let obj6_offset = data.len();
    data.extend_from_slice(b"6 0 obj\n<<\n /Type /Catalog\n /Pages 5 0 R\n>>\nendobj\n\n");

    let obj1_offset = data.len();
    data.extend_from_slice(
        b"1 0 obj\n<<\n /Type /Page\n /Parent 5 0 R\n /MediaBox [ 0 0 612 792 ]\n /Resources 3 0 R\n /Contents 2 0 R\n>>\nendobj\n\n",
    );

    let obj4_offset = data.len();
    data.extend_from_slice(
        b"4 0 obj\n<<\n /Type /Font\n /Subtype /Type1\n /Name /F1\n /BaseFont/Helvetica\n>>\nendobj\n\n",
    );

    let obj2_offset = data.len();
    data.extend_from_slice(
        b"2 0 obj\n<<\n /Length 53\n>>\nstream\ntoString\nendstream\nendobj\n\n",
    );

    let obj5_offset = data.len();
    data.extend_from_slice(
        b"5 0 obj\n<<\n /Type /Pages\n /Kids [ 1 0 R ]\n /Count 1\n>>\nendobj\n\n",
    );

    let obj3_offset = data.len();
    data.extend_from_slice(
        b"3 0 obj\n<< /ProcSet[/PDF/Text]\n /Font <</F1 4 0 R >>\n>>\nendobj\n\n",
    );

    let stream_payload_offset = data
        .windows(b"toString".len())
        .position(|window| window == b"toString")
        .expect("stream payload should exist");

    let xref_offset = data.len();
    data.extend_from_slice(b"xref\n0 7\n");
    data.extend_from_slice(format_xref_entry(0, 65535, false).as_bytes());
    data.extend_from_slice(format_xref_entry(obj1_offset.saturating_sub(8), 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(stream_payload_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj3_offset.saturating_add(4), 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj4_offset.saturating_sub(3), 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj5_offset.saturating_add(9), 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj6_offset, 0, true).as_bytes());

    data.extend_from_slice(b"trailer\n<< /Size 7 /Root 6 0 R >>\n");
    data.extend_from_slice(b"startxref\n");
    data.extend_from_slice(format!("{}\n", xref_offset.saturating_add(36)).as_bytes());
    data.extend_from_slice(b"%%EOF");

    (
        data,
        BTreeMap::from([
            (1, obj1_offset),
            (2, obj2_offset),
            (3, obj3_offset),
            (4, obj4_offset),
            (5, obj5_offset),
            (6, obj6_offset),
        ]),
    )
}

fn build_hybrid_xref_pdf() -> Vec<u8> {
    fn push_xref_stream_entry(data: &mut Vec<u8>, entry_type: u8, field2: u16, field3: u8) {
        data.push(entry_type);
        data.extend_from_slice(&field2.to_be_bytes());
        data.push(field3);
    }

    let mut data = Vec::new();
    data.extend_from_slice(b"%PDF-1.7\n");

    let obj1_offset = data.len();
    data.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    let object_2 = b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>";
    let object_3 = b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>";
    let object_2_offset = 0usize;
    let object_3_offset = object_2.len().saturating_add(1);
    let object_stream_header = format!("2 {object_2_offset} 3 {object_3_offset} ").into_bytes();
    let first = object_stream_header.len();
    let mut object_stream_data = object_stream_header;
    object_stream_data.extend_from_slice(object_2);
    object_stream_data.push(b' ');
    object_stream_data.extend_from_slice(object_3);

    let obj4_offset = data.len();
    data.extend_from_slice(
        format!(
            "4 0 obj\n<< /Type /ObjStm /N 2 /First {first} /Length {} >>\nstream\n",
            object_stream_data.len()
        )
        .as_bytes(),
    );
    data.extend_from_slice(&object_stream_data);
    data.extend_from_slice(b"\nendstream\nendobj\n");

    let obj5_offset = data.len();
    let mut xref_stream_data = Vec::new();
    push_xref_stream_entry(&mut xref_stream_data, 0, 0, u8::MAX);
    push_xref_stream_entry(&mut xref_stream_data, 1, obj1_offset as u16, 0);
    push_xref_stream_entry(&mut xref_stream_data, 2, 4, 0);
    push_xref_stream_entry(&mut xref_stream_data, 2, 4, 1);
    push_xref_stream_entry(&mut xref_stream_data, 1, obj4_offset as u16, 0);
    push_xref_stream_entry(&mut xref_stream_data, 1, obj5_offset as u16, 0);
    data.extend_from_slice(
        format!(
            "5 0 obj\n<< /Type /XRef /Size 6 /W [1 2 1] /Index [0 6] /Length {} >>\nstream\n",
            xref_stream_data.len()
        )
        .as_bytes(),
    );
    data.extend_from_slice(&xref_stream_data);
    data.extend_from_slice(b"\nendstream\nendobj\n");

    let xref_offset = data.len();
    data.extend_from_slice(b"xref\n0 2\n");
    data.extend_from_slice(format_xref_entry(0, 65535, false).as_bytes());
    data.extend_from_slice(format_xref_entry(obj1_offset, 0, true).as_bytes());
    data.extend_from_slice(b"4 2\n");
    data.extend_from_slice(format_xref_entry(obj4_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj5_offset, 0, true).as_bytes());
    data.extend_from_slice(
        format!(
            "trailer\n<< /Size 6 /Root 1 0 R /XRefStm {obj5_offset} >>\nstartxref\n{xref_offset}\n%%EOF"
        )
        .as_bytes(),
    );

    data
}

fn build_malformed_xref_stream_pdf() -> Vec<u8> {
    fn push_xref_stream_entry(data: &mut Vec<u8>, entry_type: u8, field2: usize, field3: u8) {
        data.push(entry_type);
        data.extend_from_slice(&(field2 as u16).to_be_bytes());
        data.push(field3);
    }

    let mut data = b"%PDF-1.5\n".to_vec();

    let obj1_offset = data.len();
    data.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    let object_2 = b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>";
    let object_3 = b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 6 0 R >> >> >>";
    let object_2_offset = 0usize;
    let object_3_offset = object_2.len().saturating_add(1);
    let object_stream_header = format!("2 {object_2_offset} 3 {object_3_offset} ").into_bytes();
    let first = object_stream_header.len();
    let mut object_stream_data = object_stream_header;
    object_stream_data.extend_from_slice(object_2);
    object_stream_data.push(b' ');
    object_stream_data.extend_from_slice(object_3);

    let obj4_offset = data.len();
    data.extend_from_slice(
        format!(
            "4 0 obj\n<< /Type /ObjStm /N 2 /First {first} /Length {} >>\nstream\n",
            object_stream_data.len()
        )
        .as_bytes(),
    );
    data.extend_from_slice(&object_stream_data);
    data.extend_from_slice(b"\nendstream\nendobj\n");

    let obj6_offset = data.len();
    data.extend_from_slice(
        b"6 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n",
    );

    let obj5_offset = data.len();
    let mut xref_stream_data = Vec::new();
    push_xref_stream_entry(&mut xref_stream_data, 0, 0, u8::MAX);
    push_xref_stream_entry(&mut xref_stream_data, 1, obj1_offset, 0);
    push_xref_stream_entry(&mut xref_stream_data, 2, 4, 0);
    push_xref_stream_entry(&mut xref_stream_data, 2, 4, 1);
    push_xref_stream_entry(&mut xref_stream_data, 1, obj4_offset, 0);
    push_xref_stream_entry(&mut xref_stream_data, 1, obj5_offset, 0);
    push_xref_stream_entry(&mut xref_stream_data, 1, obj6_offset, 0);
    push_xref_stream_entry(&mut xref_stream_data, 1, obj4_offset, 0);

    data.extend_from_slice(
        format!(
            "5 0 obj\n<< /Type /XRef /Size 8 /W [1 2 1] /Index [0 8] /Length {} /Root 1 0 R >>\nstream\n",
            xref_stream_data.len()
        )
        .as_bytes(),
    );
    data.extend_from_slice(&xref_stream_data);
    data.extend_from_slice(b"\nendstream\nendobj\n");
    data.extend_from_slice(format!("startxref\n{obj5_offset}\n%%EOF").as_bytes());
    data
}

fn build_malformed_incremental_xref_subsection_pdf() -> (Vec<u8>, BTreeMap<usize, usize>) {
    let mut data = Vec::new();
    data.extend_from_slice(b"%PDF-1.3\n");

    let obj1_offset = data.len();
    data.extend_from_slice(b"1 0 obj\n<< /Type /ExtGState /CA 1 >>\nendobj\n");

    let obj2_offset = data.len();
    data.extend_from_slice(b"2 0 obj\n<< /Length 0 >>\nstream\n\nendstream\nendobj\n");

    let obj3_offset = data.len();
    data.extend_from_slice(
        b"3 0 obj\n<< /Type /Page /Parent 4 0 R /MediaBox [0 0 200 50] /Resources << /ExtGState << /GS1 1 0 R >> >> /Contents 2 0 R >>\nendobj\n",
    );

    let obj4_v1_offset = data.len();
    data.extend_from_slice(b"4 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");

    let obj5_v1_offset = data.len();
    data.extend_from_slice(b"5 0 obj\n<< /Type /Catalog /Pages 4 0 R >>\nendobj\n");

    let obj6_offset = data.len();
    data.extend_from_slice(b"6 0 obj\n<< /Producer (test) >>\nendobj\n");

    let xref1_offset = data.len();
    data.extend_from_slice(b"xref\r1 7\r");
    data.extend_from_slice(
        format_xref_entry(0, 65_535, false)
            .replace('\n', "\r")
            .as_bytes(),
    );
    data.extend_from_slice(
        format_xref_entry(obj1_offset, 0, true)
            .replace('\n', "\r")
            .as_bytes(),
    );
    data.extend_from_slice(
        format_xref_entry(obj2_offset, 0, true)
            .replace('\n', "\r")
            .as_bytes(),
    );
    data.extend_from_slice(
        format_xref_entry(obj3_offset, 0, true)
            .replace('\n', "\r")
            .as_bytes(),
    );
    data.extend_from_slice(
        format_xref_entry(obj4_v1_offset, 0, true)
            .replace('\n', "\r")
            .as_bytes(),
    );
    data.extend_from_slice(
        format_xref_entry(obj5_v1_offset, 0, true)
            .replace('\n', "\r")
            .as_bytes(),
    );
    data.extend_from_slice(
        format_xref_entry(obj6_offset, 0, true)
            .replace('\n', "\r")
            .as_bytes(),
    );
    data.extend_from_slice(b"trailer\r<< /Size 7 /Root 5 0 R /Info 6 0 R >>\r");
    data.extend_from_slice(format!("startxref\r{xref1_offset}\r%%EOF\r").as_bytes());

    let obj4_v2_offset = data.len();
    data.extend_from_slice(b"4 0 obj\n<< /Type /Pages /Kids [3 0 R 8 0 R] /Count 2 >>\nendobj\n");

    let obj5_v2_offset = data.len();
    data.extend_from_slice(b"5 0 obj\n<< /Type /Catalog /Pages 4 0 R >>\nendobj\n");

    let obj8_offset = data.len();
    data.extend_from_slice(
        b"8 0 obj\n<< /Type /Page /Parent 4 0 R /MediaBox [0 0 200 50] /Contents 9 0 R >>\nendobj\n",
    );

    let obj9_offset = data.len();
    data.extend_from_slice(b"9 0 obj\n<< /Length 0 >>\nstream\n\nendstream\nendobj\n");

    let xref2_offset = data.len();
    data.extend_from_slice(b"xref\n0 1\n");
    data.extend_from_slice(format_xref_entry(0, 65_535, false).as_bytes());
    data.extend_from_slice(b"4 6\n");
    data.extend_from_slice(format_xref_entry(obj4_v2_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj5_v2_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj6_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(0, 0, false).as_bytes());
    data.extend_from_slice(format_xref_entry(obj8_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj9_offset, 0, true).as_bytes());
    data.extend_from_slice(
        format!("trailer\n<< /Size 10 /Root 5 0 R /Info 6 0 R /Prev {xref1_offset} >>\n")
            .as_bytes(),
    );
    data.extend_from_slice(format!("startxref\n{xref2_offset}\n%%EOF").as_bytes());

    (
        data,
        BTreeMap::from([
            (1, obj1_offset),
            (2, obj2_offset),
            (3, obj3_offset),
            (4, obj4_v2_offset),
            (5, obj5_v2_offset),
            (6, obj6_offset),
            (8, obj8_offset),
            (9, obj9_offset),
        ]),
    )
}

fn build_far_shifted_xref_pdf(
    object_count: usize,
    offset_delta: usize,
) -> (Vec<u8>, BTreeMap<usize, usize>) {
    let mut data = Vec::new();
    data.extend_from_slice(b"%PDF-1.4\n");

    let mut offsets = BTreeMap::new();

    for object_number in 1..object_count {
        let object_offset = data.len();
        let _ = offsets.insert(object_number, object_offset);

        let object = match object_number {
            1 => b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n".to_vec(),
            2 => b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n".to_vec(),
            3 => b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 50] /Contents 4 0 R >>\nendobj\n".to_vec(),
            4 => b"4 0 obj\n<< /Length 0 >>\nstream\n\nendstream\nendobj\n".to_vec(),
            _ => format!("{object_number} 0 obj\n<< /N {object_number} >>\nendobj\n").into_bytes(),
        };
        data.extend_from_slice(&object);
    }

    let xref_offset = data.len();
    data.extend_from_slice(format!("xref\n0 {object_count}\n").as_bytes());
    data.extend_from_slice(format_xref_entry(0, 65_535, false).as_bytes());

    for object_number in 1..object_count {
        let byte_offset = offsets
            .get(&object_number)
            .copied()
            .expect("object offset should exist");
        data.extend_from_slice(
            format_xref_entry(byte_offset.saturating_add(offset_delta), 0, true).as_bytes(),
        );
    }

    data.extend_from_slice(format!("trailer\n<< /Size {object_count} /Root 1 0 R >>\n").as_bytes());
    data.extend_from_slice(
        format!(
            "startxref\n{}\n%%EOF",
            xref_offset.saturating_add(offset_delta)
        )
        .as_bytes(),
    );

    (data, offsets)
}

#[test]
fn build_xref_table_simple() {
    let mut data = Vec::new();
    data.extend_from_slice(b"%PDF-1.7\n");

    let obj1_offset = data.len();
    data.extend_from_slice(b"1 0 obj\n<< /Type /Catalog >>\nendobj\n");

    let xref_offset = data.len();
    data.extend_from_slice(b"xref\n0 2\n");
    data.extend_from_slice(format_xref_entry(0, 65535, false).as_bytes());
    data.extend_from_slice(format_xref_entry(obj1_offset, 0, true).as_bytes());
    data.extend_from_slice(b"trailer\n<< /Size 2 /Root 1 0 R >>\n");
    data.extend_from_slice(format!("startxref\n{xref_offset}\n%%EOF").as_bytes());

    let mut parser = PdfParser::from(data.as_slice());
    let table = parser.build_xref_table().unwrap();

    assert_eq!(table.entries.len(), 2);
    assert_eq!(
        table
            .entries
            .get(&1)
            .and_then(CrossReferenceEntry::byte_offset),
        Some(obj1_offset)
    );
    assert!(table.entries.get(&0).expect("obj 0 should exist").is_free());

    let size: i64 = table
        .trailer
        .dictionary
        .get("Size")
        .expect("Size expected")
        .try_number(&PassthroughResolver)
        .unwrap();
    assert_eq!(size, 2);
}

#[test]
fn build_xref_table_falls_back_from_invalid_newer_xref() {
    let mut data = Vec::new();
    data.extend_from_slice(b"%PDF-1.7\n");

    let obj1_offset = data.len();
    data.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    let obj2_offset = data.len();
    data.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n");

    let xref1_offset = data.len();
    data.extend_from_slice(b"xref\n0 3\n");
    data.extend_from_slice(format_xref_entry(0, 65535, false).as_bytes());
    data.extend_from_slice(format_xref_entry(obj1_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj2_offset, 0, true).as_bytes());
    data.extend_from_slice(b"trailer\n<< /Size 3 /Root 1 0 R >>\n");
    data.extend_from_slice(format!("startxref\n{xref1_offset}\n%%EOF\n").as_bytes());

    let invalid_obj2_offset = obj2_offset.saturating_add(2);
    let xref2_offset = data.len();
    data.extend_from_slice(b"xref\n0 3\n");
    data.extend_from_slice(format_xref_entry(0, 65535, false).as_bytes());
    data.extend_from_slice(format_xref_entry(obj1_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(invalid_obj2_offset, 0, true).as_bytes());
    data.extend_from_slice(
        format!("trailer\n<< /Size 3 /Root 1 0 R /Prev {xref1_offset} >>\n").as_bytes(),
    );
    data.extend_from_slice(format!("startxref\n{xref2_offset}\n%%EOF").as_bytes());

    let mut parser = PdfParser::from(data.as_slice());
    let table = parser.build_xref_table().unwrap();

    assert_eq!(
        table
            .entries
            .get(&2)
            .and_then(CrossReferenceEntry::byte_offset),
        Some(obj2_offset)
    );
}

#[test]
fn build_xref_table_repairs_valid_section_with_shifted_entry_offsets() {
    let mut data = Vec::new();
    data.extend_from_slice(b"%PDF-1.1\n");

    let obj1_offset = data.len();
    data.extend_from_slice(b"1 0 obj\n<</Type/Catalog/Pages 2 0 R>>\nendobj\n");

    let obj2_offset = data.len();
    data.extend_from_slice(b"2 0 obj\n<</Type/Pages/Count 1/Kids[3 0 R]>>\nendobj\n");

    let obj3_offset = data.len();
    data.extend_from_slice(
        b"3 0 obj\n<<\n /Type/Page\n /Parent 2 0 R\n /Resources<</XObject<</SomeImage 4 0 R>>>>\n /Contents 5 0 R\n /MediaBox[0 0 500 400]\n>>\nendobj\n",
    );

    let obj4_offset = data.len();
    data.extend_from_slice(
        b"4 0 obj\n<<\n /Type/XObject\n /Subtype/Image\n /Width 1\n /Height 1\n /ColorSpace/DeviceRGB\n /BitsPerComponent 8\n /Filter/DCTDecode\n /Length 284\n>>\nstream\nplaceholder-image-data\nendstream\nendobj\n",
    );

    let obj5_offset = data.len();
    data.extend_from_slice(
        b"5 0 obj\n<</Length 14>>\nstream\n500 0 0 400 0 0 cm\n/SomeImage Do\nendstream\nendobj\n",
    );

    let xref_offset = data.len();
    data.extend_from_slice(b"xref\n0 6\n");
    data.extend_from_slice(format_xref_entry(0, 65535, false).as_bytes());
    data.extend_from_slice(format_xref_entry(obj1_offset.saturating_sub(1), 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj2_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj3_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj4_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj5_offset.saturating_sub(1), 0, true).as_bytes());
    data.extend_from_slice(b"trailer\n<</Root 1 0 R/Size 6>>\n");
    data.extend_from_slice(format!("startxref\n{xref_offset}\n%%EOF").as_bytes());

    let mut parser = PdfParser::from(data.as_slice());
    let table = parser.build_xref_table().unwrap();

    assert_eq!(
        table
            .entries
            .get(&1)
            .and_then(CrossReferenceEntry::byte_offset),
        Some(obj1_offset)
    );
    assert_eq!(
        table
            .entries
            .get(&5)
            .and_then(CrossReferenceEntry::byte_offset),
        Some(obj5_offset)
    );
}

#[test]
fn build_xref_table_recovers_missing_xref_keyword_with_subsection_header() {
    let mut data = Vec::new();
    data.extend_from_slice(b"%PDF-1.7\n");

    let obj1_offset = data.len();
    data.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    let obj2_offset = data.len();
    data.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");

    let obj3_offset = data.len();
    data.extend_from_slice(b"3 0 obj\n<< /Type /Page /Parent 2 0 R >>\nendobj\n");

    let xref_offset = data.len();
    data.extend_from_slice(b"0 4\n");
    data.extend_from_slice(format_xref_entry(0, 65535, false).as_bytes());
    data.extend_from_slice(format_xref_entry(obj1_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj2_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj3_offset, 0, true).as_bytes());
    data.extend_from_slice(b"trailer\n<< /Size 4 /Root 1 0 R >>\n");
    data.extend_from_slice(format!("startxref\n{xref_offset}\n%%EOF").as_bytes());

    let mut parser = PdfParser::from(data.as_slice());
    let table = parser.build_xref_table().unwrap();

    assert_eq!(
        table
            .entries
            .get(&1)
            .and_then(CrossReferenceEntry::byte_offset),
        Some(obj1_offset)
    );
    assert_eq!(
        table
            .entries
            .get(&2)
            .and_then(CrossReferenceEntry::byte_offset),
        Some(obj2_offset)
    );
    assert_eq!(
        table
            .entries
            .get(&3)
            .and_then(CrossReferenceEntry::byte_offset),
        Some(obj3_offset)
    );
}

#[test]
fn build_xref_table_recovers_stripped_header_offsets() {
    let mut data = Vec::new();
    data.extend_from_slice(b"%\xe2\xe3\xcf\xd3\n");

    let obj1_offset = data.len();
    data.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    let obj2_offset = data.len();
    data.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n");

    let xref_offset = data.len();
    data.extend_from_slice(b"xref\n0 3\n");
    const STRIPPED_HEADER_DELTA: usize = 9;
    data.extend_from_slice(format_xref_entry(0, 65535, false).as_bytes());
    data.extend_from_slice(
        format_xref_entry(obj1_offset + STRIPPED_HEADER_DELTA, 0, true).as_bytes(),
    );
    data.extend_from_slice(
        format_xref_entry(obj2_offset + STRIPPED_HEADER_DELTA, 0, true).as_bytes(),
    );
    data.extend_from_slice(b"trailer\n<< /Size 3 /Root 1 0 R >>\n");
    data.extend_from_slice(
        format!("startxref\n{}\n%%EOF", xref_offset + STRIPPED_HEADER_DELTA).as_bytes(),
    );

    let mut parser = PdfParser::from(data.as_slice());
    let table = parser.build_xref_table().unwrap();

    assert_eq!(
        table
            .entries
            .get(&1)
            .and_then(CrossReferenceEntry::byte_offset),
        Some(obj1_offset)
    );
    assert_eq!(
        table
            .entries
            .get(&2)
            .and_then(CrossReferenceEntry::byte_offset),
        Some(obj2_offset)
    );
}

#[test]
fn build_xref_table_recovers_startxref_inside_endstream() {
    let mut data = Vec::new();
    data.extend_from_slice(b"%PDF-1.7\n");

    let obj1_offset = data.len();
    data.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    let obj2_offset = data.len();
    data.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n");

    let obj3_offset = data.len();
    data.extend_from_slice(b"3 0 obj\n<< /Length 0 >>\nstream\n\nendstream\nendobj\n");

    let bad_startxref_offset = data
        .windows(b"endstream".len())
        .position(|window| window == b"endstream")
        .expect("fixture should contain endstream")
        .saturating_add(1);

    let xref_offset = data.len();
    data.extend_from_slice(b"xref\n0 4\n");
    data.extend_from_slice(format_xref_entry(0, 65535, false).as_bytes());
    data.extend_from_slice(format_xref_entry(obj1_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj2_offset.saturating_sub(3), 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj3_offset.saturating_sub(2), 0, true).as_bytes());
    data.extend_from_slice(b"trailer\n<< /Size 4 /Root 1 0 R >>\n");
    data.extend_from_slice(format!("startxref\n{bad_startxref_offset}\n%%EOF").as_bytes());

    let mut parser = PdfParser::from(data.as_slice());
    let table = parser.build_xref_table().unwrap();

    assert_eq!(
        table
            .entries
            .get(&1)
            .and_then(CrossReferenceEntry::byte_offset),
        Some(obj1_offset)
    );
    assert_eq!(
        table
            .entries
            .get(&2)
            .and_then(CrossReferenceEntry::byte_offset),
        Some(obj2_offset)
    );
    assert_eq!(
        table
            .entries
            .get(&3)
            .and_then(CrossReferenceEntry::byte_offset),
        Some(obj3_offset)
    );
    assert!(bad_startxref_offset < xref_offset);
}

#[test]
fn build_xref_table_recovers_nearby_xref_without_line_boundary() {
    let mut data = Vec::new();
    data.extend_from_slice(b"%PDF-1.7\n");

    let obj1_offset = data.len();
    data.extend_from_slice(b"1 0 obj\n<< /Title (Issue 10438) >>\n");

    let obj2_offset = data.len();
    data.extend_from_slice(b"2 0 obj\n<< /Type /Catalog /Pages 3 0 R >>\nendobj\n");

    let obj3_offset = data.len();
    data.extend_from_slice(b"3 0 obj\n<< /Type /Pages /Kids [4 0 R] /Count 1 >>\nendobj\n");

    let obj4_offset = data.len();
    data.extend_from_slice(
        b"4 0 obj\n<< /Type /Page /Parent 3 0 R /MediaBox [0 0 200 50] /Contents 5 0 R >>\nendobj\n",
    );

    let obj5_offset = data.len();
    data.extend_from_slice(b"5 0 obj\n<< /Length 0 >>\nstream\nendstream\nendobj");

    let bad_startxref_offset = data
        .windows(b"endstream".len())
        .position(|window| window == b"endstream")
        .expect("fixture should contain endstream")
        .saturating_add(1);

    data.extend_from_slice(b" xref\n0 6\n");
    data.extend_from_slice(format_xref_entry(0, 65535, false).as_bytes());
    data.extend_from_slice(format_xref_entry(1, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(2, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(3, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(4, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(5, 0, true).as_bytes());
    data.extend_from_slice(b"trailer\n<< /Size 6 /Root 2 0 R /Info 1 0 R >>\n");
    data.extend_from_slice(format!("startxref\n{bad_startxref_offset}\n%%EOF").as_bytes());

    let mut parser = PdfParser::from(data.as_slice());
    let table = parser.build_xref_table().unwrap();

    assert_eq!(
        table
            .entries
            .get(&1)
            .and_then(CrossReferenceEntry::byte_offset),
        Some(obj1_offset)
    );
    assert_eq!(
        table
            .entries
            .get(&2)
            .and_then(CrossReferenceEntry::byte_offset),
        Some(obj2_offset)
    );
    assert_eq!(
        table
            .entries
            .get(&3)
            .and_then(CrossReferenceEntry::byte_offset),
        Some(obj3_offset)
    );
    assert_eq!(
        table
            .entries
            .get(&4)
            .and_then(CrossReferenceEntry::byte_offset),
        Some(obj4_offset)
    );
    assert_eq!(
        table
            .entries
            .get(&5)
            .and_then(CrossReferenceEntry::byte_offset),
        Some(obj5_offset)
    );
}

#[test]
fn build_xref_table_repairs_issue139_offsets() {
    let (data, expected_offsets) = build_issue139_like_pdf();
    let mut parser = PdfParser::from(data.as_slice());
    let table = parser.build_xref_table().unwrap();

    for object_number in 1..=6 {
        let entry = table
            .entries
            .get(&object_number)
            .expect("expected normal entry");
        let expected_offset = expected_offsets
            .get(&object_number)
            .copied()
            .expect("expected offset");
        let byte_offset = entry
            .byte_offset()
            .expect("entry should have a byte offset");
        assert_eq!(byte_offset, expected_offset);
        assert_indirect_object_at_offset(data.as_slice(), byte_offset, object_number, 0);
    }
}

#[test]
fn build_xref_table_repair_ignores_numeric_array_entries() {
    let mut data = Vec::new();
    data.extend_from_slice(b"%PDF-1.4\n");

    let obj1_offset = data.len();
    data.extend_from_slice(
        b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R /OpenAction [1 0] >>\nendobj\n",
    );

    let obj2_offset = data.len();
    data.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n");

    let false_header_offset = data
        .windows(b"1 0]".len())
        .position(|window| window == b"1 0]")
        .expect("fixture should contain numeric array entry");

    let xref_offset = data.len();
    data.extend_from_slice(b"xref\n0 3\n");
    data.extend_from_slice(format_xref_entry(0, 65_535, false).as_bytes());
    data.extend_from_slice(format_xref_entry(false_header_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj2_offset, 0, true).as_bytes());
    data.extend_from_slice(b"trailer\n<< /Size 3 /Root 1 0 R >>\n");
    data.extend_from_slice(format!("startxref\n{xref_offset}\n%%EOF").as_bytes());

    let mut parser = PdfParser::from(data.as_slice());
    let table = parser.build_xref_table().unwrap();

    assert_eq!(
        table
            .entries
            .get(&1)
            .and_then(CrossReferenceEntry::byte_offset),
        Some(obj1_offset)
    );
}

#[test]
fn build_xref_table_recovers_distant_shifted_xref_offsets() {
    let (data, expected_offsets) = build_far_shifted_xref_pdf(500, 105_359);
    let mut parser = PdfParser::from(data.as_slice());
    let table = parser.build_xref_table().unwrap();

    assert!(table.entries.get(&0).expect("obj 0 should exist").is_free());

    for object_number in [1, 4, 10, 100, 499] {
        let entry = table.entries.get(&object_number).expect("expected entry");
        let expected_offset = expected_offsets
            .get(&object_number)
            .copied()
            .expect("expected offset");
        let byte_offset = entry
            .byte_offset()
            .expect("entry should have a byte offset");
        assert_eq!(byte_offset, expected_offset);
        assert_indirect_object_at_offset(data.as_slice(), byte_offset, object_number, 0);
    }
}

#[test]
fn build_xref_table_merges_hybrid_xref_stream_entries() {
    let data = build_hybrid_xref_pdf();
    let mut parser = PdfParser::from(data.as_slice());
    let table = parser.build_xref_table().unwrap();

    let pages_entry = table.entries.get(&2).expect("obj 2 should exist");
    match &pages_entry.entry_type {
        CrossReferenceEntryType::Compressed {
            object_stream_number,
            index_within_stream,
        } => {
            assert_eq!(*object_stream_number, 4);
            assert_eq!(*index_within_stream, 0);
        }
        other => panic!("expected compressed xref entry for obj 2, got {other:?}"),
    }

    let page_entry = table.entries.get(&3).expect("obj 3 should exist");
    match &page_entry.entry_type {
        CrossReferenceEntryType::Compressed {
            object_stream_number,
            index_within_stream,
        } => {
            assert_eq!(*object_stream_number, 4);
            assert_eq!(*index_within_stream, 1);
        }
        other => panic!("expected compressed xref entry for obj 3, got {other:?}"),
    }
}

#[test]
fn build_xref_table_drops_invalid_normal_entry_from_xref_stream() {
    let data = build_malformed_xref_stream_pdf();
    let mut parser = PdfParser::from(data.as_slice());
    let table = parser.build_xref_table().unwrap();

    assert_eq!(
        table
            .entries
            .get(&1)
            .and_then(CrossReferenceEntry::byte_offset),
        Some(9)
    );
    assert!(
        table
            .entries
            .get(&6)
            .and_then(CrossReferenceEntry::byte_offset)
            .is_some()
    );
    assert!(
        table
            .entries
            .get(&2)
            .expect("obj 2 should exist")
            .is_compressed()
    );
    assert!(!table.entries.contains_key(&7));
}

#[test]
fn build_xref_table_recovers_missing_xref_command() {
    let mut data = Vec::new();
    data.extend_from_slice(b"%PDF-1.1\n");

    while data.len() < 15 {
        data.push(b' ');
    }
    let obj1_offset = data.len();
    data.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    while data.len() < 70 {
        data.push(b' ');
    }
    data.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");

    while data.len() < 125 {
        data.push(b' ');
    }
    let obj3_offset = data.len();
    data.extend_from_slice(b"3 0 obj\n<< /Type /Page /Parent 2 0 R /Contents 4 0 R >>\nendobj\n");

    while data.len() < 200 {
        data.push(b' ');
    }
    data.extend_from_slice(b"4 0 obj\n<< /Length 0 >>\nstream\n\nendstream\nendobj\n");

    while data.len() < 355 {
        data.push(b' ');
    }
    let obj5_offset = data.len();
    data.extend_from_slice(b"5 0 obj\n<< /Producer (test) >>\nendobj\n");

    let xref_offset = data.len();
    data.extend_from_slice(b"0 6\n");
    data.extend_from_slice(format_xref_entry(0, 65_535, false).as_bytes());
    data.extend_from_slice(format_xref_entry(obj1_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(70, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj3_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(200, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj5_offset, 0, true).as_bytes());
    data.extend_from_slice(
        format!("trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF").as_bytes(),
    );

    let mut parser = PdfParser::from(data.as_slice());
    let table = parser.build_xref_table().unwrap();

    assert!(table.entries.get(&0).expect("obj 0 should exist").is_free());
    assert_eq!(
        table
            .entries
            .get(&1)
            .and_then(CrossReferenceEntry::byte_offset),
        Some(obj1_offset)
    );
    assert_eq!(
        table
            .entries
            .get(&3)
            .and_then(CrossReferenceEntry::byte_offset),
        Some(obj3_offset)
    );
    assert_eq!(
        table
            .entries
            .get(&5)
            .and_then(CrossReferenceEntry::byte_offset),
        Some(obj5_offset)
    );
}

#[test]
fn build_xref_table_recovers_xref_keyword_with_flat_entries() {
    let mut data = Vec::new();
    data.extend_from_slice(b"%PDF-1.4\n");

    let obj1_offset = data.len();
    data.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    let obj2_offset = data.len();
    data.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");

    let obj3_offset = data.len();
    data.extend_from_slice(b"3 0 obj\n<< /Type /Page /Parent 2 0 R /Resources << /XObject << /Im 5 0 R >> >> >>\nendobj\n");

    let obj4_offset = data.len();
    data.extend_from_slice(b"4 0 obj\n<< /Length 0 >>\nstream\n\nendstream\nendobj\n");

    let obj5_offset = data.len();
    data.extend_from_slice(b"5 0 obj\n<< /Type /XObject /Subtype /Image /Width 1 /Height 1 /ColorSpace /DeviceGray /BitsPerComponent 8 /Length 1 >>\nstream\nA\nendstream\nendobj\n");

    let xref_offset = data.len();
    data.extend_from_slice(b"xref\n");
    data.extend_from_slice(format_xref_entry(0, 65_535, false).as_bytes());
    data.extend_from_slice(format_xref_entry(obj1_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj2_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj3_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj4_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj5_offset, 0, true).as_bytes());
    data.extend_from_slice(
        format!("trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF").as_bytes(),
    );

    let mut parser = PdfParser::from(data.as_slice());
    let table = parser.build_xref_table().unwrap();

    assert_eq!(
        table
            .entries
            .get(&5)
            .and_then(CrossReferenceEntry::byte_offset),
        Some(obj5_offset)
    );
}

#[test]
fn build_xref_table_normalizes_malformed_incremental_subsection_numbering() {
    let (data, expected_offsets) = build_malformed_incremental_xref_subsection_pdf();
    let mut parser = PdfParser::from(data.as_slice());
    let table = parser.build_xref_table().unwrap();

    for (object_number, expected_offset) in expected_offsets {
        let entry = table
            .entries
            .get(&object_number)
            .expect("expected recovered entry");
        assert_eq!(entry.byte_offset(), Some(expected_offset));
    }
}
