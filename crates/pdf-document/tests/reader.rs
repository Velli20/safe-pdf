#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::as_conversions
)]

use pdf_document::{diagnostic::PdfReadDiagnosticKind, error::PdfReaderError, reader::PdfReader};
use pdf_object::error::ObjectError;

fn format_xref_entry(offset: usize, generation: u16, used: bool) -> String {
    let kind = if used { 'n' } else { 'f' };
    format!("{:010} {:05} {} \n", offset, generation, kind)
}

fn build_issue139_like_pdf() -> Vec<u8> {
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

    let _obj2_offset = data.len();
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

    data
}

fn build_issue1293_like_pdf() -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(b"%PDF-1.7\n");

    let obj1_offset = data.len();
    data.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    let obj2_offset = data.len();
    data.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");

    let obj4_offset = data.len();
    data.extend_from_slice(b"4 0 obj\n<< >>\nstream\nBT ET\nendstream\nendobj\n");

    let obj3_offset = data.len();
    data.extend_from_slice(
        b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << >> /Contents 4 0 R >>\nendobj\n",
    );

    let xref_offset = data.len();
    data.extend_from_slice(b"xref\n0 5\n");
    data.extend_from_slice(format_xref_entry(0, 65535, false).as_bytes());
    data.extend_from_slice(format_xref_entry(obj1_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj2_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj3_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj4_offset, 0, true).as_bytes());

    data.extend_from_slice(b"trailer\n<< /Size 5 /Root 1 0 R >>\n");
    data.extend_from_slice(b"startxref\n");
    data.extend_from_slice(format!("{}\n", xref_offset).as_bytes());
    data.extend_from_slice(b"%%EOF");

    data
}

fn build_pdfbox4352_like_pdf() -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(b"%PDF-1.7\n");

    let obj1_offset = data.len();
    data.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    let obj2_offset = data.len();
    data.extend_from_slice(b"2 0 obj\n<< /Count 1 /Kids [3 0 R] /Type /Pages >>\nendobj\n");

    let obj3_offset = data.len();
    data.extend_from_slice(
        b"3 0 obj\n<< /Contents 4 0 R /MediaBox [0 0 200 50] /Parent 2 0 R /Type /Page >>\nendobj\n",
    );

    let obj4_offset = data.len();
    data.extend_from_slice(b"4 0 obj\n<< /Length 4 /Filter /FlateDecode >>\nstream\n");
    data.extend_from_slice(&[0, 1, 2, 3]);
    data.extend_from_slice(b"\nendstream\nendobj\n");

    let obj5_offset = data.len();
    data.extend_from_slice(b"5 0 obj\nE< /Filter /Standard /V 5 /R 6 /Length 256 >>\nendobj\n");

    let xref_offset = data.len();
    data.extend_from_slice(b"xref\n0 6\n");
    data.extend_from_slice(format_xref_entry(0, 65_535, false).as_bytes());
    data.extend_from_slice(format_xref_entry(obj1_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj2_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj3_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj4_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj5_offset, 0, true).as_bytes());
    data.extend_from_slice(b"trailer\n<< /Size 6 /Root 1 0 R /Encrypt 5 0 R >>\n");
    data.extend_from_slice(format!("startxref\n{xref_offset}\n%%EOF").as_bytes());

    data
}

fn build_issue10438_like_pdf() -> Vec<u8> {
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
    data.extend_from_slice(b"5 0 obj\n<< /Length 0 >>\nstream\n\nendstream\nendobj");

    let bad_startxref_offset = data
        .windows(b"endstream".len())
        .position(|window| window == b"endstream")
        .expect("fixture should contain endstream")
        .saturating_add(1);

    data.extend_from_slice(b" xref\n0 6\n");
    data.extend_from_slice(format_xref_entry(0, 65535, false).as_bytes());
    data.extend_from_slice(format_xref_entry(obj1_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj2_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj3_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj4_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj5_offset, 0, true).as_bytes());
    data.extend_from_slice(b"trailer\n<< /Size 6 /Root 2 0 R /Info 1 0 R >>\n");
    data.extend_from_slice(format!("startxref\n{bad_startxref_offset}\n%%EOF").as_bytes());

    data
}

fn build_missing_xref_command_pdf() -> Vec<u8> {
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
    let _obj2_offset = data.len();
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

    data
}

fn build_xobject_image_pdf() -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(b"%PDF-1.7\n");

    let obj1_offset = data.len();
    data.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    let obj2_offset = data.len();
    data.extend_from_slice(
        b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 1 1] >>\nendobj\n",
    );

    let obj3_offset = data.len();
    data.extend_from_slice(
        b"3 0 obj\n<< /Type /Page /Parent 2 0 R /Resources 5 0 R /MediaBox [0 0 1 1] >>\nendobj\n",
    );

    let obj4_offset = data.len();
    data.extend_from_slice(
        b"4 0 obj\n<< /Type /XObject /Subtype /Image /Width 1 /Height 1 /ColorSpace /DeviceGray /BitsPerComponent 8 /Length 1 /SMask /None >>\nstream\nA\nendstream\nendobj\n",
    );

    let obj5_offset = data.len();
    data.extend_from_slice(b"5 0 obj\n<< /XObject << /Im1 4 0 R >> >>\nendobj\n");

    let xref_offset = data.len();
    data.extend_from_slice(b"xref\n0 6\n");
    data.extend_from_slice(format_xref_entry(0, 65535, false).as_bytes());
    data.extend_from_slice(format_xref_entry(obj1_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj2_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj3_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj4_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj5_offset, 0, true).as_bytes());

    data.extend_from_slice(b"trailer\n<< /Size 6 /Root 1 0 R >>\n");
    data.extend_from_slice(b"startxref\n");
    data.extend_from_slice(format!("{}\n", xref_offset).as_bytes());
    data.extend_from_slice(b"%%EOF");

    data
}

fn build_xobject_with_malformed_dimensions_pdf() -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(b"%PDF-1.7\n");

    let obj1_offset = data.len();
    data.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    let obj2_offset = data.len();
    data.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");

    let obj3_offset = data.len();
    data.extend_from_slice(
        b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 50] /Resources << /XObject << /I1 4 0 R >> >> /Contents 5 0 R >>\nendobj\n",
    );

    let obj4_offset = data.len();
    data.extend_from_slice(
        b"4 0 obj\n<< /Type /XObject /Subtype /Image /Width /Height /ColorSpace /DeviceGray /BitsPerComponent 8 /Length 1 >>\nstream\nA\nendstream\nendobj\n",
    );

    let obj5_offset = data.len();
    data.extend_from_slice(
        b"5 0 obj\n<< /Length 12 >>\nstream\nq\n/I1 Do\nQ\n\nendstream\nendobj\n",
    );

    let xref_offset = data.len();
    data.extend_from_slice(b"xref\n0 6\n");
    data.extend_from_slice(format_xref_entry(0, 65_535, false).as_bytes());
    data.extend_from_slice(format_xref_entry(obj1_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj2_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj3_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj4_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj5_offset, 0, true).as_bytes());

    data.extend_from_slice(b"trailer\n<< /Size 6 /Root 1 0 R >>\n");
    data.extend_from_slice(b"startxref\n");
    data.extend_from_slice(format!("{}\n", xref_offset).as_bytes());
    data.extend_from_slice(b"%%EOF");

    data
}

fn build_xobject_image_with_flat_xref_rows_pdf() -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(b"%PDF-1.4\n");

    let obj1_offset = data.len();
    data.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    let obj2_offset = data.len();
    data.extend_from_slice(
        b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 1 1] >>\nendobj\n",
    );

    let obj3_offset = data.len();
    data.extend_from_slice(
        b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 1 1] /Contents 4 0 R /Resources << /XObject << /Im 5 0 R >> >> >>\nendobj\n",
    );

    let obj4_offset = data.len();
    data.extend_from_slice(b"4 0 obj\n<< /Length 6 >>\nstream\n/Im Do\nendstream\nendobj\n");

    let obj5_offset = data.len();
    data.extend_from_slice(
        b"5 0 obj\n<< /Type /XObject /Subtype /Image /Width 1 /Height 1 /ColorSpace /DeviceGray /BitsPerComponent 8 /Length 1 >>\nstream\nA\nendstream\nendobj\n",
    );

    let xref_offset = data.len();
    data.extend_from_slice(b"xref\n");
    data.extend_from_slice(format_xref_entry(0, 65_535, false).as_bytes());
    data.extend_from_slice(format_xref_entry(obj1_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj2_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj3_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj4_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj5_offset, 0, true).as_bytes());

    data.extend_from_slice(b"trailer\n<< /Size 6 /Root 1 0 R >>\n");
    data.extend_from_slice(b"startxref\n");
    data.extend_from_slice(format!("{}\n", xref_offset).as_bytes());
    data.extend_from_slice(b"%%EOF");

    data
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
    fn push_xref_stream_entry(data: &mut Vec<u8>, entry_type: u8, field2: u16, field3: u8) {
        data.push(entry_type);
        data.extend_from_slice(&field2.to_be_bytes());
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
    push_xref_stream_entry(&mut xref_stream_data, 1, obj1_offset as u16, 0);
    push_xref_stream_entry(&mut xref_stream_data, 2, 4, 0);
    push_xref_stream_entry(&mut xref_stream_data, 2, 4, 1);
    push_xref_stream_entry(&mut xref_stream_data, 1, obj4_offset as u16, 0);
    push_xref_stream_entry(&mut xref_stream_data, 1, obj5_offset as u16, 0);
    push_xref_stream_entry(&mut xref_stream_data, 1, obj6_offset as u16, 0);
    push_xref_stream_entry(&mut xref_stream_data, 1, obj4_offset as u16, 0);

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

fn build_malformed_incremental_xref_subsection_pdf() -> Vec<u8> {
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

    data
}

#[test]
fn test_encrypted_document_detection() {
    let mut data = Vec::new();
    data.extend_from_slice(b"%PDF-1.7\n");

    let obj1_offset = data.len();
    data.extend_from_slice(b"1 0 obj\n<< /Type /Catalog >>\nendobj\n");

    let obj2_offset = data.len();
    data.extend_from_slice(b"2 0 obj\n");
    data.extend_from_slice(b"<< /Filter /Standard /V 2 /R 3 /Length 128 ");
    data.extend_from_slice(b"/O <00000000000000000000000000000000");
    data.extend_from_slice(b"00000000000000000000000000000000> ");
    data.extend_from_slice(b"/U <00000000000000000000000000000000");
    data.extend_from_slice(b"00000000000000000000000000000000> ");
    data.extend_from_slice(b"/P -1 >>\n");
    data.extend_from_slice(b"endobj\n");

    let xref_offset = data.len();
    data.extend_from_slice(b"xref\n0 3\n");
    data.extend_from_slice(format_xref_entry(0, 65535, false).as_bytes());
    data.extend_from_slice(format_xref_entry(obj1_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj2_offset, 0, true).as_bytes());

    data.extend_from_slice(b"trailer\n<< /Size 3 /Root 1 0 R /Encrypt 2 0 R >>\n");
    data.extend_from_slice(b"startxref\n");
    data.extend_from_slice(format!("{}\n", xref_offset).as_bytes());
    data.extend_from_slice(b"%%EOF");

    let reader = PdfReader;
    let result = reader.read_from_bytes(&data, None);
    assert!(matches!(
        result,
        Err(PdfReaderError::ObjectError(
            ObjectError::MissingRequiredKey { ref key }
        )) if key == "ID"
    ));
}

#[test]
fn test_encrypted_document_v4_aes() {
    let mut data = Vec::new();
    data.extend_from_slice(b"%PDF-1.7\n");

    let obj1_offset = data.len();
    data.extend_from_slice(b"1 0 obj\n<< /Type /Catalog >>\nendobj\n");

    let obj2_offset = data.len();
    data.extend_from_slice(b"2 0 obj\n");
    data.extend_from_slice(b"<< /Filter /Standard /V 4 /R 4 /Length 128 ");
    data.extend_from_slice(b"/O <00000000000000000000000000000000");
    data.extend_from_slice(b"00000000000000000000000000000000> ");
    data.extend_from_slice(b"/U <00000000000000000000000000000000");
    data.extend_from_slice(b"00000000000000000000000000000000> ");
    data.extend_from_slice(b"/P -1 /EncryptMetadata false >>\n");
    data.extend_from_slice(b"endobj\n");

    let xref_offset = data.len();
    data.extend_from_slice(b"xref\n0 3\n");
    data.extend_from_slice(format_xref_entry(0, 65535, false).as_bytes());
    data.extend_from_slice(format_xref_entry(obj1_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj2_offset, 0, true).as_bytes());

    data.extend_from_slice(b"trailer\n<< /Size 3 /Root 1 0 R /Encrypt 2 0 R >>\n");
    data.extend_from_slice(b"startxref\n");
    data.extend_from_slice(format!("{}\n", xref_offset).as_bytes());
    data.extend_from_slice(b"%%EOF");

    let reader = PdfReader;
    let result = reader.read_from_bytes(&data, None);
    assert!(result.is_err());
}

#[test]
fn test_unencrypted_document_loads_normally() {
    let mut data = Vec::new();
    data.extend_from_slice(b"%PDF-1.7\n");

    let obj1_offset = data.len();
    data.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    let obj2_offset = data.len();
    data.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n");

    let xref_offset = data.len();
    data.extend_from_slice(b"xref\n0 3\n");
    data.extend_from_slice(format_xref_entry(0, 65535, false).as_bytes());
    data.extend_from_slice(format_xref_entry(obj1_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj2_offset, 0, true).as_bytes());

    data.extend_from_slice(b"trailer\n<< /Size 3 /Root 1 0 R >>\n");
    data.extend_from_slice(b"startxref\n");
    data.extend_from_slice(format!("{}\n", xref_offset).as_bytes());
    data.extend_from_slice(b"%%EOF");

    let reader = PdfReader;
    let result = reader.read_from_bytes(&data, None);
    assert!(
        result.is_ok(),
        "Unencrypted document should load: {:?}",
        result.err()
    );

    let doc = result.unwrap();
    assert_eq!(doc.page_count(), 0);
}

#[test]
fn test_headerless_document_loads_normally() {
    let mut data = Vec::new();
    data.extend_from_slice(b"%\xe2\xe3\xcf\xd3\n");

    let obj1_offset = data.len();
    data.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    let obj2_offset = data.len();
    data.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");

    let obj3_offset = data.len();
    data.extend_from_slice(
        b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] >>\nendobj\n",
    );

    let xref_offset = data.len();
    data.extend_from_slice(b"xref\n0 4\n");
    data.extend_from_slice(format_xref_entry(0, 65535, false).as_bytes());
    data.extend_from_slice(format_xref_entry(obj1_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj2_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj3_offset, 0, true).as_bytes());

    data.extend_from_slice(b"trailer\n<< /Size 4 /Root 1 0 R >>\n");
    data.extend_from_slice(b"startxref\n");
    data.extend_from_slice(format!("{}\n", xref_offset).as_bytes());
    data.extend_from_slice(b"%%EOF");

    let reader = PdfReader;
    let result = reader.read_from_bytes(&data, None);
    assert!(result.is_ok());
    assert_eq!(result.unwrap().page_count(), 1);
}

#[test]
fn test_headerless_document_with_shifted_xref_offsets_loads_normally() {
    let mut data = Vec::new();
    data.extend_from_slice(b"%\xe2\xe3\xcf\xd3\n");

    let obj1_offset = data.len();
    data.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    let obj2_offset = data.len();
    data.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");

    let obj3_offset = data.len();
    data.extend_from_slice(
        b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] >>\nendobj\n",
    );

    let xref_offset = data.len();
    data.extend_from_slice(b"xref\n0 4\n");
    const STRIPPED_HEADER_DELTA: usize = 9;
    data.extend_from_slice(format_xref_entry(0, 65535, false).as_bytes());
    data.extend_from_slice(
        format_xref_entry(obj1_offset + STRIPPED_HEADER_DELTA, 0, true).as_bytes(),
    );
    data.extend_from_slice(
        format_xref_entry(obj2_offset + STRIPPED_HEADER_DELTA, 0, true).as_bytes(),
    );
    data.extend_from_slice(
        format_xref_entry(obj3_offset + STRIPPED_HEADER_DELTA, 0, true).as_bytes(),
    );
    data.extend_from_slice(b"trailer\n<< /Size 4 /Root 1 0 R >>\n");
    data.extend_from_slice(
        format!("startxref\n{}\n%%EOF", xref_offset + STRIPPED_HEADER_DELTA).as_bytes(),
    );

    let reader = PdfReader;
    let result = reader.read_from_bytes(&data, None);
    assert!(result.is_ok());
    assert_eq!(result.unwrap().page_count(), 1);
}

#[test]
fn test_issue139_pdf_loads_normally() {
    let reader = PdfReader;
    let data = build_issue139_like_pdf();
    let result = reader.read_from_bytes(&data, None);
    assert!(
        result.is_ok(),
        "issue139.pdf should load after xref recovery: {:?}",
        result.err()
    );
    assert_eq!(result.unwrap().page_count(), 1);
}

#[test]
fn test_issue1293_pdf_loads_normally() {
    let reader = PdfReader;
    let data = build_issue1293_like_pdf();
    let result = reader.read_from_bytes(&data, None);
    assert!(
        result.is_ok(),
        "issue1293r.pdf should load with missing /Length recovery: {:?}",
        result.err()
    );
    let doc = result.unwrap();
    assert_eq!(doc.page_count(), 1);
    assert!(doc.get_page(0).is_some());
}

#[test]
fn test_pdfbox4352_pdf_loads_normally() {
    let reader = PdfReader;
    let data = build_pdfbox4352_like_pdf();
    let result = reader.read_with_report(&data, None);
    assert!(
        result.is_ok(),
        "PDFBOX-4352-style PDF should load despite malformed optional encryption object: {:?}",
        result.err()
    );
    let report = result.unwrap();
    assert_eq!(report.document().page_count(), 1);
    assert!(report.document().get_page(0).is_some());
    assert!(
        report
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.kind == PdfReadDiagnosticKind::MalformedEncryption)
    );
}

#[test]
fn test_issue10438_pdf_loads_normally() {
    let reader = PdfReader;
    let data = build_issue10438_like_pdf();
    let result = reader.read_from_bytes(&data, None);
    assert!(
        result.is_ok(),
        "issue10438_reduced.pdf should load after xref recovery: {:?}",
        result.err()
    );
    let doc = result.unwrap();
    assert_eq!(doc.page_count(), 1);
    assert!(doc.get_page(0).is_some());
}

#[test]
fn test_missing_xref_command_pdf_loads_normally() {
    let reader = PdfReader;
    let data = build_missing_xref_command_pdf();
    let result = reader.read_from_bytes(&data, None);
    assert!(
        result.is_ok(),
        "xref_command_missing.pdf should load after malformed xref recovery: {:?}",
        result.err()
    );
    let doc = result.unwrap();
    assert_eq!(doc.page_count(), 1);
    assert!(doc.get_page(0).is_some());
}

#[test]
fn test_xobject_image_pdf_loads_normally() {
    let reader = PdfReader;
    let data = build_xobject_image_pdf();
    let result = reader.read_from_bytes(&data, None);
    assert!(
        result.is_ok(),
        "xobject-image.pdf should load after traditional xref entry repair: {:?}",
        result.err()
    );
    let doc = result.unwrap();
    assert_eq!(doc.page_count(), 1);
    assert!(doc.get_page(0).is_some());
}

#[test]
fn test_xobject_with_malformed_dimensions_loads_normally() {
    let data = build_xobject_with_malformed_dimensions_pdf();
    let doc = PdfReader
        .read_from_bytes(&data, None)
        .expect("a malformed image should not prevent the document from loading");
    let page = doc.get_page(0).expect("the page should remain available");
    let xobject = page
        .resources
        .as_ref()
        .and_then(|resources| resources.xobject("I1"));

    assert!(matches!(
        xobject,
        Some(pdf_resources::resource::Resource::UnavailableImage)
    ));
    assert!(page.contents.is_some());
}

#[test]
fn test_xobject_image_with_flat_xref_rows_pdf_loads_normally() {
    let reader = PdfReader;
    let data = build_xobject_image_with_flat_xref_rows_pdf();
    let result = reader.read_from_bytes(&data, None);
    assert!(
        result.is_ok(),
        "xobject image with xref keyword and flat rows should load: {:?}",
        result.err()
    );
    let doc = result.unwrap();
    assert_eq!(doc.page_count(), 1);
    assert!(doc.get_page(0).is_some());
}

#[test]
fn test_malformed_incremental_xref_subsection_pdf_loads_normally() {
    let reader = PdfReader;
    let data = build_malformed_incremental_xref_subsection_pdf();
    let result = reader.read_from_bytes(&data, None);
    assert!(
        result.is_ok(),
        "malformed incremental xref subsection should load after recovery: {:?}",
        result.err()
    );
    let doc = result.unwrap();
    assert_eq!(doc.page_count(), 2);
    assert!(doc.get_page(0).is_some());
    assert!(doc.get_page(1).is_some());
}

#[test]
fn test_hybrid_xref_pdf_loads_normally() {
    let reader = PdfReader;
    let data = build_hybrid_xref_pdf();
    let result = reader.read_from_bytes(&data, None);
    assert!(
        result.is_ok(),
        "hybrid-reference PDF should load after /XRefStm merge: {:?}",
        result.err()
    );
    let doc = result.unwrap();
    assert_eq!(doc.page_count(), 1);
    assert!(doc.get_page(0).is_some());
}

#[test]
fn test_stream_with_indirect_length_resolves() {
    let mut data = Vec::new();
    data.extend_from_slice(b"%PDF-1.7\n");

    let obj3_offset = data.len();
    data.extend_from_slice(b"3 0 obj\n5\nendobj\n");

    let obj4_offset = data.len();
    data.extend_from_slice(b"4 0 obj\n<< /Length 3 0 R >>\nstream\nHello\nendstream\nendobj\n");

    let obj1_offset = data.len();
    data.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    let obj2_offset = data.len();
    data.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n");

    let xref_offset = data.len();
    data.extend_from_slice(b"xref\n0 5\n");
    data.extend_from_slice(format_xref_entry(0, 65535, false).as_bytes());
    data.extend_from_slice(format_xref_entry(obj1_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj2_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj3_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj4_offset, 0, true).as_bytes());

    data.extend_from_slice(b"trailer\n<< /Size 5 /Root 1 0 R >>\n");
    data.extend_from_slice(b"startxref\n");
    data.extend_from_slice(format!("{}\n", xref_offset).as_bytes());
    data.extend_from_slice(b"%%EOF");

    let reader = PdfReader;
    let result = reader.read_from_bytes(&data, None);
    assert!(
        result.is_ok(),
        "Stream with indirect /Length should resolve: {:?}",
        result.err()
    );
    assert_eq!(result.unwrap().page_count(), 0);
}

#[test]
fn test_stream_with_compressed_indirect_length_resolves() {
    fn push_xref_stream_entry(data: &mut Vec<u8>, entry_type: u8, field2: usize, field3: u8) {
        data.push(entry_type);
        data.extend_from_slice(&u16::try_from(field2).expect("offset fits").to_be_bytes());
        data.push(field3);
    }

    let mut data = Vec::new();
    data.extend_from_slice(b"%PDF-1.7\n");

    let obj1_offset = data.len();
    data.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    let obj2_offset = data.len();
    data.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n");

    let obj4_offset = data.len();
    data.extend_from_slice(b"4 0 obj\n<< /Length 3 0 R >>\nstream\nHello\nendstream\nendobj\n");

    let object_stream_data = b"3 0 5";
    let obj5_offset = data.len();
    data.extend_from_slice(
        format!(
            "5 0 obj\n<< /Type /ObjStm /N 1 /First 4 /Length {} >>\nstream\n",
            object_stream_data.len()
        )
        .as_bytes(),
    );
    data.extend_from_slice(object_stream_data);
    data.extend_from_slice(b"\nendstream\nendobj\n");

    let obj6_offset = data.len();
    let mut xref_stream_data = Vec::new();
    push_xref_stream_entry(&mut xref_stream_data, 0, 0, u8::MAX);
    push_xref_stream_entry(&mut xref_stream_data, 1, obj1_offset, 0);
    push_xref_stream_entry(&mut xref_stream_data, 1, obj2_offset, 0);
    push_xref_stream_entry(&mut xref_stream_data, 2, 5, 0);
    push_xref_stream_entry(&mut xref_stream_data, 1, obj4_offset, 0);
    push_xref_stream_entry(&mut xref_stream_data, 1, obj5_offset, 0);
    push_xref_stream_entry(&mut xref_stream_data, 1, obj6_offset, 0);
    data.extend_from_slice(
        format!(
            "6 0 obj\n<< /Type /XRef /Size 7 /W [1 2 1] /Root 1 0 R /Length {} >>\nstream\n",
            xref_stream_data.len()
        )
        .as_bytes(),
    );
    data.extend_from_slice(&xref_stream_data);
    data.extend_from_slice(b"\nendstream\nendobj\n");
    data.extend_from_slice(format!("startxref\n{obj6_offset}\n%%EOF").as_bytes());

    let reader = PdfReader;
    let result = reader.read_from_bytes(&data, None);
    assert!(
        result.is_ok(),
        "stream with compressed indirect /Length should resolve: {:?}",
        result.err()
    );
    assert_eq!(result.unwrap().page_count(), 0);
}

#[test]
fn test_unresolvable_reference_returns_error() {
    let mut data = Vec::new();
    data.extend_from_slice(b"%PDF-1.7\n");

    let obj4_offset = data.len();
    data.extend_from_slice(b"4 0 obj\n<< /Length 99 0 R >>\nstream\nHello\nendstream\nendobj\n");

    let obj1_offset = data.len();
    data.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    let obj2_offset = data.len();
    data.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n");

    let xref_offset = data.len();
    data.extend_from_slice(b"xref\n0 5\n");
    data.extend_from_slice(format_xref_entry(0, 65535, false).as_bytes());
    data.extend_from_slice(format_xref_entry(obj1_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj2_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(0, 0, false).as_bytes());
    data.extend_from_slice(format_xref_entry(obj4_offset, 0, true).as_bytes());

    data.extend_from_slice(b"trailer\n<< /Size 5 /Root 1 0 R >>\n");
    data.extend_from_slice(b"startxref\n");
    data.extend_from_slice(format!("{}\n", xref_offset).as_bytes());
    data.extend_from_slice(b"%%EOF");

    let reader = PdfReader;
    let result = reader.read_from_bytes(&data, None);
    assert!(matches!(
        result,
        Err(PdfReaderError::UnresolvedObjects {
            count: 1,
            first_offset,
        }) if first_offset == obj4_offset
    ));
}

#[test]
fn test_image_xobject_smask_name_none_loads_normally() {
    let reader = PdfReader;
    let data = build_xobject_image_pdf();
    let result = reader.read_from_bytes(&data, None);
    assert!(
        result.is_ok(),
        "Image XObject with /SMask /None should load: {:?}",
        result.err()
    );
    assert_eq!(result.unwrap().page_count(), 1);
}

#[test]
fn test_malformed_xref_stream_pdf_loads_normally() {
    let reader = PdfReader;
    let data = build_malformed_xref_stream_pdf();
    let result = reader.read_from_bytes(&data, None);
    assert!(
        result.is_ok(),
        "xref stream with one bad unused normal entry should still load: {:?}",
        result.err()
    );
    let doc = result.unwrap();
    assert_eq!(doc.page_count(), 1);
    assert!(doc.get_page(0).is_some());
}

#[test]
fn test_cyclic_page_tree_does_not_overflow() {
    let mut data = Vec::new();
    data.extend_from_slice(b"%PDF-1.7\n");

    let obj1_offset = data.len();
    data.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    let obj2_offset = data.len();
    data.extend_from_slice(
        b"2 0 obj\n<< /Type /Pages /Kids [6 0 R 3 0 R] /Count 2 /MediaBox [0 0 595 842] >>\nendobj\n",
    );

    let obj3_offset = data.len();
    data.extend_from_slice(
        b"3 0 obj\n<< /Type /Pages /Kids [4 0 R] /Count 1 /MediaBox [0 0 595 842] >>\nendobj\n",
    );

    let obj4_offset = data.len();
    data.extend_from_slice(
        b"4 0 obj\n<< /Type /Pages /Kids [5 0 R] /Count 1 /MediaBox [0 0 595 842] >>\nendobj\n",
    );

    let obj5_offset = data.len();
    data.extend_from_slice(
        b"5 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 595 842] >>\nendobj\n",
    );

    let obj6_offset = data.len();
    data.extend_from_slice(
        b"6 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] >>\nendobj\n",
    );

    let xref_offset = data.len();
    data.extend_from_slice(b"xref\n0 7\n");
    data.extend_from_slice(format_xref_entry(0, 65535, false).as_bytes());
    data.extend_from_slice(format_xref_entry(obj1_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj2_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj3_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj4_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj5_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj6_offset, 0, true).as_bytes());

    data.extend_from_slice(b"trailer\n<< /Size 7 /Root 1 0 R >>\n");
    data.extend_from_slice(b"startxref\n");
    data.extend_from_slice(format!("{}\n", xref_offset).as_bytes());
    data.extend_from_slice(b"%%EOF");

    let reader = PdfReader;
    let result = reader.read_from_bytes(&data, None);
    assert!(
        result.is_ok(),
        "Cyclic page tree should not cause an error: {:?}",
        result.err()
    );

    let doc = result.unwrap();
    assert_eq!(doc.page_count(), 1);
    let page = doc.get_page(0).expect("page 0 should exist");
    let mb = page
        .media_box
        .as_ref()
        .expect("page should inherit MediaBox from parent /Pages");
    assert_eq!(mb.right, 595.0);
    assert_eq!(mb.bottom, 842.0);
}

#[test]
fn test_sampled_function_shading_loads_normally() {
    let mut data = Vec::new();
    data.extend_from_slice(b"%PDF-1.7\n");

    let obj1_offset = data.len();
    data.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    let obj2_offset = data.len();
    data.extend_from_slice(
        b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 100 100] >>\nendobj\n",
    );

    let obj3_offset = data.len();
    data.extend_from_slice(
        b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Resources 4 0 R >>\nendobj\n",
    );

    let obj4_offset = data.len();
    data.extend_from_slice(b"4 0 obj\n<< /Shading << /Sh1 5 0 R >> >>\nendobj\n");

    let obj5_offset = data.len();
    data.extend_from_slice(
        b"5 0 obj\n<< /ShadingType 2 /ColorSpace /DeviceGray /Coords [0 0 100 0] /Function 6 0 R >>\nendobj\n",
    );

    let obj6_offset = data.len();
    data.extend_from_slice(
        b"6 0 obj\n<< /FunctionType 0 /Domain [0 1] /Range [0 1] /Size [4] /BitsPerSample 8 /Length 4 >>\nstream\n",
    );
    data.extend_from_slice(&[0x00, 0x55, 0xAA, 0xFF]);
    data.extend_from_slice(b"\nendstream\nendobj\n");

    let xref_offset = data.len();
    data.extend_from_slice(b"xref\n0 7\n");
    data.extend_from_slice(format_xref_entry(0, 65535, false).as_bytes());
    data.extend_from_slice(format_xref_entry(obj1_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj2_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj3_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj4_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj5_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj6_offset, 0, true).as_bytes());

    data.extend_from_slice(b"trailer\n<< /Size 7 /Root 1 0 R >>\n");
    data.extend_from_slice(b"startxref\n");
    data.extend_from_slice(format!("{}\n", xref_offset).as_bytes());
    data.extend_from_slice(b"%%EOF");

    let reader = PdfReader;
    let result = reader.read_from_bytes(&data, None);
    assert!(
        result.is_ok(),
        "PDF with sampled function shading should load: {:?}",
        result.err()
    );
    assert_eq!(result.unwrap().page_count(), 1);
}

#[test]
fn test_sampled_function_shading_with_single_name_array_color_space_loads_normally() {
    let mut data = Vec::new();
    data.extend_from_slice(b"%PDF-1.7\n");

    let obj1_offset = data.len();
    data.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    let obj2_offset = data.len();
    data.extend_from_slice(
        b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 100 100] >>\nendobj\n",
    );

    let obj3_offset = data.len();
    data.extend_from_slice(
        b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Resources 4 0 R >>\nendobj\n",
    );

    let obj4_offset = data.len();
    data.extend_from_slice(
        b"4 0 obj\n<< /Shading << /Sh1 5 0 R >> /ColorSpace << /CS1 [ /DeviceGray ] >> >>\nendobj\n",
    );

    let obj5_offset = data.len();
    data.extend_from_slice(
        b"5 0 obj\n<< /ShadingType 2 /ColorSpace [ /DeviceGray ] /Coords [0 0 100 0] /Function 6 0 R >>\nendobj\n",
    );

    let obj6_offset = data.len();
    data.extend_from_slice(
        b"6 0 obj\n<< /FunctionType 0 /Domain [0 1] /Range [0 1] /Size [4] /BitsPerSample 8 /Length 4 >>\nstream\n",
    );
    data.extend_from_slice(&[0x00, 0x55, 0xAA, 0xFF]);
    data.extend_from_slice(b"\nendstream\nendobj\n");

    let xref_offset = data.len();
    data.extend_from_slice(b"xref\n0 7\n");
    data.extend_from_slice(format_xref_entry(0, 65535, false).as_bytes());
    data.extend_from_slice(format_xref_entry(obj1_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj2_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj3_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj4_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj5_offset, 0, true).as_bytes());
    data.extend_from_slice(format_xref_entry(obj6_offset, 0, true).as_bytes());

    data.extend_from_slice(b"trailer\n<< /Size 7 /Root 1 0 R >>\n");
    data.extend_from_slice(b"startxref\n");
    data.extend_from_slice(format!("{}\n", xref_offset).as_bytes());
    data.extend_from_slice(b"%%EOF");

    let reader = PdfReader;
    let result = reader.read_from_bytes(&data, None);
    assert!(
        result.is_ok(),
        "PDF with single-name array color space shading should load: {:?}",
        result.err()
    );
    assert_eq!(result.unwrap().page_count(), 1);
}
