use crate::{error::ParserError, parser::PdfParser, traits::TrailerParser};
use pdf_object::trailer::Trailer;

impl TrailerParser for PdfParser<'_> {
    type ErrorType = ParserError;

    /// Parses the PDF file trailer from the current position in the input stream.
    ///
    /// According to the PDF 1.7 Specification (Section 7.5.5 "File Trailer"):
    /// The trailer of a PDF file enables a conforming reader to quickly find the
    /// cross-reference table and certain special objects. Conforming readers should
    /// read a PDF file from its end. The last line of the file shall contain only the
    /// end-of-file marker, `%%EOF`.
    ///
    /// # Format
    ///
    /// The trailer consists of:
    /// 1. The keyword `trailer`.
    /// 2. A dictionary object (enclosed in `<<` and `>>`) containing key-value pairs.
    ///    Common and important keys include:
    ///    - `/Size`: (Integer, Required) The total number of entries in the file’s
    ///      cross-reference table.
    ///    - `/Root`: (Indirect Reference, Required) The catalog dictionary for the PDF document.
    ///    - `/Prev`: (Integer, Optional) The byte offset of the previous cross-reference section.
    ///    - `/Encrypt`: (Indirect Reference, Optional) The document’s encryption dictionary.
    ///    - `/Info`: (Indirect Reference, Optional) The document’s information dictionary.
    ///    - `/ID`: (Array, Optional) An array of two byte strings constituting a file identifier.
    /// 3. The keyword `startxref`.
    /// 4. An integer representing the byte offset from the beginning of the file to the
    ///    `xref` keyword of the last (or only) cross-reference section.
    /// 5. The end-of-file marker `%%EOF` (This parser expects `%%EOF` to be handled
    ///    separately after the trailer is parsed).
    ///
    /// # Example Input
    ///
    /// ```text
    /// trailer
    /// << /Size 22 /Root 2 0 R /Info 1 0 R >>
    /// startxref
    /// 1879
    /// %%EOF
    /// ```
    ///
    /// # Returns
    ///
    /// A `Trailer` object containing the parsed dictionary and the `startxref` offset,
    /// or a `ParserError` if the trailer is malformed (e.g., missing keywords,
    /// invalid dictionary, or missing offset).
    fn parse_trailer(&mut self) -> Result<Trailer, Self::ErrorType> {
        const TRAILER_KEYWORD: &[u8] = b"trailer";
        const START_XREF_KEYWORD: &[u8] = b"startxref";

        // Expect the `trailer` keyword.
        self.read_keyword(TRAILER_KEYWORD)?;

        // Try parse dictionary object.
        let dictionary = self.parse_object()?.try_dictionary()?.to_owned();

        self.read_end_of_line_marker()?;

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
    use pdf_object::ObjectVariant;

    use super::*;

    #[test]
    fn test_parse_valid_trailer() {
        let input = b"trailer\n<< /Size 22 /Root 1 0 R >>\nstartxref\n187\n%%EOF";
        let mut parser = PdfParser::from(input.as_slice());

        let trailer = parser.parse_trailer().unwrap();
        assert_eq!(
            trailer.dictionary.get("Root").unwrap(),
            &ObjectVariant::Reference(1)
        );
    }
}
