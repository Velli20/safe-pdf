use pdf_object::version::Version;
use pdf_tokenizer::PdfToken;

use crate::{ParseObject, PdfParser, error::ParserError};

impl ParseObject<Version> for PdfParser<'_> {
    /// Parses the PDF file header from the current position in the input stream.
    ///
    /// According to the PDF 1.7 Specification (Section 7.5.2 "File Header"):
    /// The first line of a PDF file shall be a header identifying the version of
    /// the PDF specification to which the file conforms.
    ///
    /// # Format
    ///
    /// - The header must start with the 5 characters `%PDF-`.
    /// - This is followed by a version number of the form `major.minor`.
    ///   - `major` and `minor` are integers. For example, for PDF 1.7,
    ///     `major` is 1 and `minor` is 7.
    /// - The header line must be terminated by an end-of-line (EOL) marker.
    ///   The EOL marker can be a carriage return (CR), a line feed (LF), or
    ///   a CR followed by an LF.
    /// - The header line should not contain any other characters, except that
    ///   versions of PDF later than 1.4 may have a comment after the EOL marker
    ///   on the first line (this parser currently expects only the version and EOL).
    ///
    /// # Example Inputs
    ///
    /// ```text
    /// %PDF-1.7
    /// ```
    /// (Followed by an EOL marker like `\n` or `\r\n`)
    ///
    /// # Returns
    ///
    /// A `Version` object containing the parsed major and minor version numbers,
    /// or a `ParserError` if the header is malformed (e.g., missing `%PDF-` prefix,
    /// invalid version format, or missing EOL).
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
