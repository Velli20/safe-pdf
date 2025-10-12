use pdf_object::version::Version;
use pdf_tokenizer::{PdfToken, error::TokenizerError};
use thiserror::Error;

use crate::{parser::PdfParser, traits::HeaderParser};

#[derive(Debug, PartialEq, Error)]
pub enum HeaderError {
    #[error("Tokenizer error: {0}")]
    TokenizerError(#[from] TokenizerError),
    #[error("Invalid PDF header prefix: expected '%PDF-', found '{0}'")]
    InvalidPrefix(String),
    #[error("Invalid version format in PDF header: expected 'major.minor', found '{0}'")]
    InvalidVersionFormat(String),
    #[error("Failed to parse major version number '{0}': {1}")]
    InvalidMajorVersion(String, #[source] std::num::ParseIntError),
    #[error("Failed to parse minor version number '{0}': {1}")]
    InvalidMinorVersion(String, #[source] std::num::ParseIntError),
    #[error("Missing end-of-line marker after PDF header")]
    MissingEOL,
}

impl HeaderParser for PdfParser<'_> {
    type ErrorType = HeaderError;
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
    fn parse_header(&mut self) -> Result<Version, HeaderError> {
        self.tokenizer.expect(PdfToken::Percent)?;

        const PDF_HEADER: &[u8] = b"PDF-";

        // Read up to the EOL, but don't consume EOL yet.
        // We need to check the prefix first.
        let current_pos = self.tokenizer.position;
        let line_bytes = self.tokenizer.read_while_u8(|b| b != b'\n' && b != b'\r');

        if !line_bytes.starts_with(PDF_HEADER) {
            return Err(HeaderError::InvalidPrefix(
                String::from_utf8_lossy(line_bytes).into_owned(),
            ));
        }

        // Extract the version part (after "PDF-")
        let version_str = String::from_utf8_lossy(&line_bytes[PDF_HEADER.len()..]);

        // Split the version number into major and minor parts.
        let parts: Vec<&str> = version_str.split('.').collect();
        if parts.len() != 2 {
            return Err(HeaderError::InvalidVersionFormat(version_str.into_owned()));
        }

        let major_str = parts[0];
        let minor_str = parts[1];

        let major = major_str
            .parse::<u8>()
            .map_err(|e| HeaderError::InvalidMajorVersion(major_str.to_string(), e))?;

        let minor = minor_str
            .parse::<u8>()
            .map_err(|e| HeaderError::InvalidMinorVersion(minor_str.to_string(), e))?;

        // Now that we've parsed the version, consume the EOL from the original line.
        // Reset position to where EOL reading should start.
        self.tokenizer.position = current_pos
            .checked_add(line_bytes.len())
            .ok_or(HeaderError::MissingEOL)?;

        self.read_end_of_line_marker()
            .map_err(|_| HeaderError::MissingEOL)?;

        Ok(Version::new(major, minor))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_header_valid() {
        let input = b"%PDF-1.7\n";
        let mut parser = PdfParser::from(input.as_slice());
        let version: Result<Version, HeaderError> = parser.parse_header();
        let version = version.unwrap();
        assert_eq!(version.major(), 1);
        assert_eq!(version.minor(), 7);
    }

    #[test]
    fn test_parse_header_invalid_format() {
        let input = b"%PDF-1.x";
        let mut parser = PdfParser::from(input.as_slice());
        let result: Result<Version, HeaderError> = parser.parse_header();
        assert!(result.is_err());
    }
}
