use pdf_object::ObjectVariant;
use pdf_parser::PdfParser;

#[test]
fn works() {
    const INPUT: &[u8] = b"3 0 obj<</Type/Pages/Count 1/Kids[ 4 0 R]>>\nendobj\n";
    let mut parser = PdfParser::from(INPUT);

    let object = parser.parse_object().unwrap();
    if let ObjectVariant::IndirectObject(object) = &object {
        assert_eq!(object.object_number, 3);
        assert_eq!(object.generation_number, 0);
    } else {
        panic!("Expected IndirectObject, got {:?}", object);
    }
}
