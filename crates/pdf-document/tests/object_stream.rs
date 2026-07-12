#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::as_conversions
)]

use pdf_document::reader::PdfReader;

fn push_xref_stream_entry(data: &mut Vec<u8>, entry_type: u8, field2: usize, field3: u8) {
    data.push(entry_type);
    data.extend_from_slice(
        &u16::try_from(field2)
            .expect("xref field2 should fit in 16 bits for this fixture")
            .to_be_bytes(),
    );
    data.push(field3);
}

fn build_object_stream_pdf() -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(b"%PDF-1.7\n");

    let obj1_offset = data.len();
    data.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    let pages_object = b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>";
    let page_object = b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>";
    let pages_offset = 0usize;
    let page_offset = pages_object.len().saturating_add(1);

    let object_stream_header = format!("2 {pages_offset} 3 {page_offset} ").into_bytes();
    let first = object_stream_header.len();
    let mut object_stream_data = object_stream_header;
    object_stream_data.extend_from_slice(pages_object);
    object_stream_data.push(b' ');
    object_stream_data.extend_from_slice(page_object);

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
    push_xref_stream_entry(&mut xref_stream_data, 1, obj1_offset, 0);
    push_xref_stream_entry(&mut xref_stream_data, 2, 4, 0);
    push_xref_stream_entry(&mut xref_stream_data, 2, 4, 1);
    push_xref_stream_entry(&mut xref_stream_data, 1, obj4_offset, 0);
    push_xref_stream_entry(&mut xref_stream_data, 1, obj5_offset, 0);
    data.extend_from_slice(
        format!(
            "5 0 obj\n<< /Type /XRef /Size 6 /W [1 2 1] /Index [0 6] /Root 1 0 R /Length {} >>\nstream\n",
            xref_stream_data.len()
        )
        .as_bytes(),
    );
    data.extend_from_slice(&xref_stream_data);
    data.extend_from_slice(b"\nendstream\nendobj\n");

    data.extend_from_slice(format!("startxref\n{obj5_offset}\n%%EOF").as_bytes());
    data
}

#[test]
fn compressed_page_tree_loads_through_the_reader() {
    let reader = PdfReader;
    let data = build_object_stream_pdf();

    let document = reader
        .read_from_bytes(&data, None)
        .expect("document should load");

    assert_eq!(document.page_count(), 1);
    assert!(document.get_page(0).is_some());
}
