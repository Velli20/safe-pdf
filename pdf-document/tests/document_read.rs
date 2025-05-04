use pdf_document::PdfDocument;
use pdf_object::{Value, comment, version::Version};
use pdf_parser::{ParseObject, PdfParser, header};

#[test]
fn works() {
    const INPUT: &[u8] = include_bytes!("assets/test.pdf");
    let document = PdfDocument::from(INPUT);
    assert_eq!(document.version.major(), 1);
    assert_eq!(document.version.minor(), 4);

    for object in &document.objects {
        println!("{:?}", object);
    }
}
