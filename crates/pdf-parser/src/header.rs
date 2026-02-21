use pdf_object::version::Version;
use pdf_tokenizer::PdfToken;
use thiserror::Error;

use crate::{error::ParserError, parser::PdfParser};

#[derive(Debug, PartialEq, Error)]
pub enum HeaderError {
    #[error("Invalid PDF header prefix: expected '%PDF-', found '{0}'")]
    InvalidPrefix(String),
    #[error("Invalid version format in PDF header: expected 'major.minor', found '{0}'")]
    InvalidVersionFormat(String),
    #[error("Failed to parse major version number '{0}': {1}")]
    InvalidMajorVersion(String, #[source] std::num::ParseIntError),
    #[error("Failed to parse minor version number '{0}': {1}")]
    InvalidMinorVersion(String, #[source] std::num::ParseIntError),
}

impl PdfParser<'_> {
    /// Parses the PDF file header from the current position in the input stream.
    ///
    /// # Returns
    ///
    /// A `Version` object containing the parsed major and minor version numbers,
    /// or a `ParserError` if the header is malformed.
    pub fn parse_header(&mut self) -> Result<Version, ParserError> {
        self.tokenizer.expect(PdfToken::Percent)?;

        const PDF_HEADER: &[u8] = b"PDF-";

        // Read up to the EOL, but don't consume EOL yet.
        let line_bytes = self.tokenizer.read_while_u8(|b| b != b'\n' && b != b'\r');
        if !line_bytes.starts_with(PDF_HEADER) {
            return Err(HeaderError::InvalidPrefix(
                String::from_utf8_lossy(line_bytes).into_owned(),
            )
            .into());
        }

        // Extract the version part (after "PDF-").
        let version_bytes = line_bytes.strip_prefix(PDF_HEADER).ok_or_else(|| {
            HeaderError::InvalidPrefix(String::from_utf8_lossy(line_bytes).into_owned())
        })?;
        let version_str = String::from_utf8_lossy(version_bytes);

        // Split the version number into major and minor parts.
        let vs: &str = version_str.as_ref();
        let (major_str, minor_str) = vs
            .split_once('.')
            .ok_or_else(|| HeaderError::InvalidVersionFormat(vs.to_string()))?;

        let major = major_str
            .parse::<u8>()
            .map_err(|e| HeaderError::InvalidMajorVersion(major_str.to_string(), e))?;

        let minor = minor_str
            .parse::<u8>()
            .map_err(|e| HeaderError::InvalidMinorVersion(minor_str.to_string(), e))?;

        self.try_read_end_of_line_marker()?;

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
        let version = parser.parse_header();
        let version = version.unwrap();
        assert_eq!(version.major(), 1);
        assert_eq!(version.minor(), 7);
    }

    #[test]
    fn test_parse_header_invalid_format() {
        let input = b"%PDF-1.x";
        let mut parser = PdfParser::from(input.as_slice());
        let result = parser.parse_header();
        assert!(result.is_err());
    }
}
