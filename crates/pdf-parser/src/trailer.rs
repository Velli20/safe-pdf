use crate::{error::ParserError, parser::PdfParser};
use pdf_object::{
    error::ObjectError, object_resolver::ObjectResolver, object_variant::ObjectVariant,
    trailer::Trailer,
};

impl PdfParser<'_> {
    /// Parses the PDF file trailer from the current position in the input stream.
    ///
    /// # Returns
    ///
    /// A `Trailer` object containing the parsed dictionary or a `ParserError`
    /// if the trailer is malformed.
    pub fn parse_trailer(&mut self, objects: &dyn ObjectResolver) -> Result<Trailer, ParserError> {
        const TRAILER_KEYWORD: &[u8] = b"trailer";
        const START_XREF_KEYWORD: &[u8] = b"startxref";

        // Expect the `trailer` keyword.
        self.read_keyword(TRAILER_KEYWORD)?;

        // Try parse dictionary object.
        let dictionary = self.parse_object(objects)?;

        let dictionary = match dictionary {
            ObjectVariant::Dictionary(value) => value,
            other => return Err(ObjectError::TypeMismatch("Dictionary", other.name()).into()),
        };

        self.skip_whitespace_and_comments();

        // A trailer located via /Prev may be followed by later body content or another xref
        // section rather than an immediate startxref footer. Preserve a usable startxref
        // offset when present, but retain the trailer dictionary when its footer is absent
        // or malformed so callers can recover the cross-reference data independently.
        let mark = self.tokenizer.position;
        let offset = if self.read_keyword(START_XREF_KEYWORD).is_ok() {
            self.read_number::<usize>(true).ok()
        } else {
            self.tokenizer.position = mark;
            None
        };

        Ok(Trailer::new(Box::new(dictionary), offset))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use pdf_object::{object_resolver::PassthroughResolver, object_variant::ObjectVariant};

    use super::*;

    #[test]
    fn test_parse_valid_trailer() {
        let input = b"trailer\n<< /Size 22 /Root 1 0 R >>\nstartxref\n187\n%%EOF";
        let mut parser = PdfParser::from(input.as_slice());

        let trailer = parser.parse_trailer(&PassthroughResolver).unwrap();
        assert_eq!(
            trailer.dictionary.get(b"Root").unwrap(),
            &ObjectVariant::Reference(1)
        );
    }

    #[test]
    fn test_parse_trailer_tolerates_spacing_before_startxref() {
        let input = b"trailer\n<< /Size 22 /Root 1 0 R >> \nstartxref\n187\n%%EOF";
        let mut parser = PdfParser::from(input.as_slice());

        let trailer = parser.parse_trailer(&PassthroughResolver).unwrap();
        assert_eq!(
            trailer.dictionary.get(b"Size").unwrap(),
            &ObjectVariant::Integer(22)
        );
        assert_eq!(trailer.offset, Some(187));
    }

    #[test]
    fn test_parse_trailer_tolerates_crlf_and_comments_before_startxref() {
        let input = b"trailer\r\n<< /Size 22 /Root 1 0 R >> %comment\r\nstartxref\r\n187\r\n%%EOF";
        let mut parser = PdfParser::from(input.as_slice());

        let trailer = parser.parse_trailer(&PassthroughResolver).unwrap();
        assert_eq!(
            trailer.dictionary.get(b"Root").unwrap(),
            &ObjectVariant::Reference(1)
        );
        assert_eq!(trailer.offset, Some(187));
    }

    #[test]
    fn test_parse_trailer_allows_missing_startxref_for_prev_sections() {
        let input = b"trailer\n<< /Size 22 /Root 1 0 R /Prev 99 >>\nxref\n0 0\n";
        let mut parser = PdfParser::from(input.as_slice());

        let trailer = parser.parse_trailer(&PassthroughResolver).unwrap();
        assert_eq!(
            trailer.dictionary.get(b"Root").unwrap(),
            &ObjectVariant::Reference(1)
        );
        assert_eq!(trailer.offset, None);
    }

    #[test]
    fn test_parse_trailer_returns_none_for_invalid_startxref_values() {
        let inputs = [
            b"trailer\n<< /Size 22 /Root 1 0 R >>\nstartxref\n".as_slice(),
            b"trailer\n<< /Size 22 /Root 1 0 R >>\nstartxref\ninvalid".as_slice(),
            b"trailer\n<< /Size 22 /Root 1 0 R >>\nstartxref\n999999999999999999999999999999999999999999999999"
                .as_slice(),
        ];

        for input in inputs {
            let mut parser = PdfParser::from(input);
            let trailer = parser.parse_trailer(&PassthroughResolver).unwrap();

            assert_eq!(trailer.offset, None);
            assert_eq!(
                trailer.dictionary.get(b"Root"),
                Some(&ObjectVariant::Reference(1))
            );
        }
    }

    #[test]
    fn test_parse_trailer_preserves_zero_startxref_value() {
        let input = b"trailer\n<< /Size 1 /Root 1 0 R >>\nstartxref\n0\n%%EOF";
        let mut parser = PdfParser::from(input.as_slice());

        let trailer = parser.parse_trailer(&PassthroughResolver).unwrap();

        assert_eq!(trailer.offset, Some(0));
    }

    #[test]
    fn test_build_xref_table_handles_trailer_spacing_before_startxref() {
        let mut data = Vec::new();
        data.extend_from_slice(b"%PDF-1.7\n");

        let obj1_offset = data.len();
        data.extend_from_slice(b"1 0 obj\n<< /Type /Catalog >>\nendobj\n");

        let xref1_offset = data.len();
        data.extend_from_slice(b"xref\n0 2\n");
        data.extend_from_slice(b"0000000000 65535 f \n");
        data.extend_from_slice(format!("{:010} {:05} n \n", obj1_offset, 0).as_bytes());
        data.extend_from_slice(b"trailer\n<< /Size 2 /Root 1 0 R >>\n");
        data.extend_from_slice(b"startxref\n");
        data.extend_from_slice(format!("{}\n", xref1_offset).as_bytes());
        data.extend_from_slice(b"%%EOF\n");

        let obj1_v2_offset = data.len();
        data.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Version /V2 >>\nendobj\n");

        let xref2_offset = data.len();
        data.extend_from_slice(b"xref\n0 2\n");
        data.extend_from_slice(b"0000000000 65535 f \n");
        data.extend_from_slice(format!("{:010} {:05} n \n", obj1_v2_offset, 0).as_bytes());
        data.extend_from_slice(b"trailer\n<< /Size 2 /Root 1 0 R /Prev ");
        data.extend_from_slice(format!("{xref1_offset}").as_bytes());
        data.extend_from_slice(b" >> \r\n");
        data.extend_from_slice(b"startxref\r\n");
        data.extend_from_slice(format!("{}\r\n", xref2_offset).as_bytes());
        data.extend_from_slice(b"%%EOF");

        let parser = PdfParser::from(data.as_slice());
        let table = parser.build_xref_table().unwrap();

        let entry = table.entries.get(&1).unwrap();
        assert_eq!(entry.byte_offset(), Some(obj1_v2_offset));
        assert_eq!(
            table
                .trailer
                .dictionary
                .get(b"Prev")
                .unwrap()
                .try_number::<usize>(&PassthroughResolver)
                .unwrap(),
            xref1_offset
        );
    }
}
