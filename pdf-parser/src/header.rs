use pdf_object::version::Version;
use pdf_tokenizer::PdfToken;

use crate::{ParseObject, PdfParser, error::ParserError};

impl ParseObject<Version> for PdfParser<'_> {
    /// Parses the PDF header from the current position in the input stream.
    ///
    /// # Parsing Rules
    ///
    /// According to the PDF 1.7 Specification, Section 7.5.2:
    /// - The header must appear within the first 1024 bytes of the file.
    /// - It must begin with the exact character sequence `%PDF-`.
    /// - Immediately after `%PDF-`, the version number must follow, in the format `major.minor` (e.g., `1.7`).
    /// - The major and minor version numbers must consist of digits separated by a single period (`.`).
    /// - After the version number, optional whitespace or line breaks may appear.
    ///
    /// - The header consists of the characters %PDF- followed by a version number of
    /// the form 1.N, where N is a digit between 0 and 7.
    /// - The specification states that the header must be the very first line of the file,
    /// starting at byte 0
    fn parse(&mut self) -> Result<Version, ParserError> {
        self.tokenizer.expect(PdfToken::Percent).unwrap();

        const PDF_HEADER: &[u8] = b"PDF-";

        // Read the PDF header prefix
        let literal = self.tokenizer.read_while_u8(|b| b != b'\n');
        if !literal.starts_with(PDF_HEADER) {
            return Err(ParserError::InvalidHeader);
        }

        // Remove the `PDF-`` prefix
        let literal = String::from_utf8_lossy(&literal[PDF_HEADER.len()..]);

        // Split the version number into major and minor parts.
        let parts = literal.split('.').collect::<Vec<_>>();
        if parts.len() != 2 {
            return Err(ParserError::InvalidHeader);
        }

        let major = parts[0]
            .parse::<u8>()
            .map_err(|_| ParserError::InvalidHeader)?;

        let minor = parts[1]
            .parse::<u8>()
            .map_err(|_| ParserError::InvalidHeader)?;

        self.read_end_of_line_marker()?;

        Ok(Version::new(major, minor))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_header_valid() {
        let input = b"%PDF-1.7\n";
        let mut parser = PdfParser::from(input.as_slice());
        let version: Result<Version, ParserError> = parser.parse();
        let version = version.unwrap();
        assert_eq!(version.major(), 1);
        assert_eq!(version.minor(), 7);
    }

    #[test]
    fn test_parse_header_invalid_format() {
        let input = b"%PDF-1.x";
        let mut parser = PdfParser::from(input.as_slice());
        let result: Result<Version, ParserError> = parser.parse();
        assert!(result.is_err());
    }
}
