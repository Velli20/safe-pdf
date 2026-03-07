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

        self.try_read_end_of_line_marker();

        // Read the `startxref` keyword.
        self.read_keyword(START_XREF_KEYWORD)?;

        // Read the offset of the xref section.
        let offset = self.read_number::<usize>(true)?;

        Ok(Trailer::new(dictionary, offset))
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
            trailer.dictionary.get("Root").unwrap(),
            &ObjectVariant::Reference(1)
        );
    }
}
