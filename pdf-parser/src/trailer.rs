use crate::{ParseObject, PdfParser, error::ParserError};
use pdf_object::{Value, trailer::Trailer};

#[derive(Debug, PartialEq)]
pub enum TrailerError {
    InvalidKeyword(String),
    InvalidSize,
    InvalidRoot,
    InvalidPrev,
    InvalidEncrypt,
    InvalidID,
    InvalidInfo,
    MissingOffset,
    MissingDictionary,
}

impl std::fmt::Display for TrailerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TrailerError::InvalidKeyword(keyword) => {
                write!(f, "Invalid trailer keyword: {}", keyword)
            }
            TrailerError::InvalidSize => write!(f, "Invalid /Size in trailer"),
            TrailerError::InvalidRoot => write!(f, "Invalid /Root in trailer"),
            TrailerError::InvalidPrev => write!(f, "Invalid /Prev in trailer"),
            TrailerError::InvalidEncrypt => write!(f, "Invalid /Encrypt in trailer"),
            TrailerError::InvalidID => write!(f, "Invalid /ID in trailer"),
            TrailerError::InvalidInfo => write!(f, "Invalid /Info in trailer"),
            TrailerError::MissingOffset => write!(f, "Missing offset in trailer"),
            TrailerError::MissingDictionary => write!(f, "Missing dictionary object in trailer"),
        }
    }
}

impl ParseObject<Trailer> for PdfParser<'_> {
    /// Parses the trailer section at the end of a PDF file according to the PDF 1.7 specification.
    ///
    /// # Syntax Rules for `trailer` Object:
    ///
    /// 1. The trailer object begins with the keyword `trailer` followed by a whitespace character.
    /// 2. This is immediately followed by a dictionary object enclosed in double angle brackets (`<< ... >>`).
    /// 3. The dictionary **must** include the following keys:
    ///    - `/Size` (required): An integer representing the total number of entries in the cross-reference table.
    ///    - `/Root` (required): A reference to the document catalog dictionary (`/Catalog`).
    /// 4. The dictionary **may** include the following optional keys:
    ///    - `/Prev`: A byte offset to the previous cross-reference section (used in incremental updates).
    ///    - `/Encrypt`: A reference to the encryption dictionary, if the document is encrypted.
    ///    - `/ID`: An array of two byte strings that uniquely identify the file.
    ///    - `/Info`: A reference to the document information dictionary.
    /// 5. The trailer dictionary is always followed by:
    ///    - The `startxref` keyword on a new line.
    ///    - An integer offset (in bytes) indicating the beginning of the xref table.
    ///    - The `%%EOF` marker to indicate the end of the file.
    ///
    /// # Notes:
    ///
    /// - Whitespace, comments, and line breaks must be handled according to PDF lexical conventions.
    /// - The trailer may appear more than once in a file if incremental updates have been applied.
    /// - The last `trailer` in the file (after the last `xref` section) is considered the authoritative one.
    ///
    /// # Example input
    ///
    /// ```text
    /// trailer
    /// <<
    ///   /Size 22 % Total objects are 0-21
    ///   /Root 1 0 R
    ///   /Prev 12345 % Offset of previous xref section (if any)
    /// >>
    /// startxref
    /// 512 % Offset of this xref section
    /// %%EOF
    /// ```
    fn parse(&mut self) -> Result<Trailer, ParserError> {
        const TRAILER_KEYWORD: &[u8] = b"trailer";
        const START_XREF_KEYWORD: &[u8] = b"startxref";

        // Expect the `trailer` keyword.
        self.read_keyword(TRAILER_KEYWORD)?;

        // Try parse dictionary object.
        let dictionary = match self.parse_object()? {
            Value::Dictionary(dictionary) => dictionary,
            _ => return Err(ParserError::from(TrailerError::MissingDictionary)),
        };

        self.read_end_of_line_marker()?;

        // Read the `startxref` keyword.
        self.read_keyword(START_XREF_KEYWORD)?;

        // Read the offset of the xref section.
        let offset = self.read_number::<u32>(TrailerError::MissingOffset)?;

        Ok(Trailer::new(dictionary))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_trailer() {
        let input = b"trailer\n<< /Size 22 /Root 1 0 R >>\nstartxref\n187\n%%EOF";
        let mut parser = PdfParser::from(input.as_slice());

        let trailer: Trailer = parser.parse().unwrap();

        assert_eq!(trailer.dictionary.get_number("Size").unwrap(), 22);
        // assert_eq!(trailer.dictionary.get("Root").unwrap(), &Value::Reference(1, 0));
    }
}
