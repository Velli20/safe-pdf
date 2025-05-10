use pdf_document::PdfDocument;

#[test]
fn works() {
    const INPUT: &[u8] = include_bytes!("assets/test4.pdf");
    let document = PdfDocument::from(INPUT).unwrap();
    assert_eq!(document.version.major(), 1);
    assert_eq!(document.version.minor(), 4);
}
