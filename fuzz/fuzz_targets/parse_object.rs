#![no_main]

use libfuzzer_sys::fuzz_target;
use pdf_parser::PdfParser;

fuzz_target!(|data: &[u8]| {
    if data.is_empty() {
        return;
    }
    let mut parser = PdfParser::from(data);
    let _ = parser.parse_object();
});
