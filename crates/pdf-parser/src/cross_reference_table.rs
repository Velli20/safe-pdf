use pdf_object::cross_reference_table::{
    CrossReferenceEntry, CrossReferenceStatus, CrossReferenceTable,
};
use pdf_tokenizer::PdfToken;
use thiserror::Error;

use crate::{error::ParserError, parser::PdfParser, traits::CrossReferenceTableParser};

/// Represents an error that can occur while parsing a cross-reference table.
#[derive(Debug, PartialEq, Error)]
pub enum CrossReferenceTableError {
    #[error("Invalid cross-reference status character: '{0}'")]
    InvalidCrossReferenceStatus(char),
    #[error("Missing status in cross-reference entry")]
    MissingStatus,
    #[error("Missing one or more table entries. Expected {0} entries, but found {1}")]
    MissingTableEntries(usize, usize),
    #[error("Too many entries in cross-reference table")]
    TooManyEntries,
}

impl CrossReferenceTableParser for PdfParser<'_> {
    type ErrorType = ParserError;

    /// Parses a cross-reference (xref) table from a PDF 1.7 document.
    ///
    /// According to the PDF 1.7 specification, section 7.5.4, a traditional
    /// cross-reference table consists of one or more sections that map object numbers
    /// to their byte offsets within the file. This enables efficient random access to
    /// indirect objects.
    ///
    /// # Format
    ///
    /// A cross-reference table begins with the keyword `xref`, followed by one or more sections.
    ///
    /// Each section starts with a line of the form:
    /// ```text
    /// start_obj count
    /// ```
    /// - `start_obj`: the first object number in the section.
    /// - `count`: the number of entries that follow.
    ///
    /// Each entry is exactly 20 bytes, consisting of:
    /// ```text
    /// 0000000000 00000 n\r\n
    /// ^         ^     ^
    /// |         |     └─ usage indicator: `n` (in use) or `f` (free)
    /// |         └─────── 5-digit generation number (0-padded)
    /// └─────────────── 10-digit byte offset (0-padded)
    /// ```
    /// - Each line must end with either LF (`\n`) or CRLF (`\r\n`).
    /// - The first entry (object 0) is always free and has generation number 65535.
    ///
    /// # Notes
    ///
    /// - All entries are fixed-width (20 bytes).
    /// - Multiple sections may exist (e.g., after incremental updates).
    /// - The `/Prev` key in the trailer may point to earlier xref tables.
    ///
    /// # Example input
    ///
    /// ```text
    /// xref
    /// 0 3
    /// 0000000000 65535 f
    /// 0000000017 00000 n
    /// 0000000081 00000 n
    /// ```
    fn parse_cross_reference_table(&mut self) -> Result<CrossReferenceTable, Self::ErrorType> {
        const XREF_KEYWORD: &[u8] = b"xref";

        // Expect the `xref` keyword.
        self.read_keyword(XREF_KEYWORD)?;
        self.skip_whitespace();

        let mut total_number_of_entries = 0_usize;
        let mut first_object_number = None;
        let mut entries = Vec::new();
        loop {
            // Read the first object number.
            if let Some(PdfToken::Number(_)) = self.tokenizer.peek() {
                let first_object_number_in_section = self.read_number::<usize>(true)?;
                if first_object_number.is_none() {
                    first_object_number = Some(first_object_number_in_section);
                }
            }

            // Read the number of objects.
            let number_of_objects = self.read_number::<u32>(true)?;

            // Read the entries.
            for _ in 0..number_of_objects {
                total_number_of_entries = total_number_of_entries
                    .checked_add(1)
                    .ok_or(CrossReferenceTableError::TooManyEntries)?;

                // Read the byte offset.
                let byte_offset = self.read_number::<usize>(true)?;

                // Read the generation number.
                let generation_number = self.read_number::<usize>(true)?;

                // Read the status.
                if let Some(PdfToken::Alphabetic(e)) = self.tokenizer.read() {
                    let status = CrossReferenceStatus::from_byte(e).ok_or_else(|| {
                        CrossReferenceTableError::InvalidCrossReferenceStatus(char::from(e))
                    })?;
                    entries.push(CrossReferenceEntry::new(
                        byte_offset,
                        generation_number,
                        status,
                    ));
                } else {
                    return Err(CrossReferenceTableError::MissingStatus.into());
                }
                self.skip_whitespace();
            }

            // If the next token is not a number, we are done reading entries.
            if !matches!(self.tokenizer.peek(), Some(PdfToken::Number(_))) {
                if entries.len() != total_number_of_entries {
                    return Err(CrossReferenceTableError::MissingTableEntries(
                        total_number_of_entries,
                        entries.len(),
                    )
                    .into());
                }
                break;
            }
        }

        Ok(CrossReferenceTable::new(
            first_object_number.unwrap_or(0),
            total_number_of_entries,
            entries,
        ))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_xref_section() {
        let data = b"xref\n0 2\n0000000000 65535 f\n0000000017 00000 n\n";
        let mut parser = PdfParser::from(data.as_slice());

        let result = parser.parse_cross_reference_table();
        assert!(result.is_ok());

        let table = result.unwrap();
        assert_eq!(table.first_object_number, 0);
        assert_eq!(table.number_of_entries, 2);
        assert_eq!(table.entries.len(), 2);

        assert_eq!(table.entries[0].byte_offset, 0);
        assert_eq!(table.entries[0].generation_number, 65535);
        assert_eq!(table.entries[0].status, CrossReferenceStatus::Free);

        assert_eq!(table.entries[1].byte_offset, 17);
        assert_eq!(table.entries[1].generation_number, 0);
        assert_eq!(table.entries[1].status, CrossReferenceStatus::Normal);
    }

    #[test]
    fn test_parse_missing_entries() {
        let data = b"xref\n0 2\n0000000000 65535 f\n";
        let mut parser = PdfParser::from(data.as_slice());

        let result = parser.parse_cross_reference_table();
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_empty_xref_section() {
        let data = b"xref\n0 0\n";
        let mut parser = PdfParser::from(data.as_slice());

        let result = parser.parse_cross_reference_table();
        assert!(result.is_ok());

        let table = result.unwrap();
        assert_eq!(table.first_object_number, 0);
        assert_eq!(table.number_of_entries, 0);
        assert!(table.entries.is_empty());
    }

    #[test]
    fn test_parse_multiple_sections() {
        let data = b"xref\n00 2\n
        0000000000 65535 f
        0000000017 00000 n
        4 2
        0000001000 00000 n
        0000001100 00000 n
        ";

        let mut parser = PdfParser::from(data.as_slice());

        let result = parser.parse_cross_reference_table();
        assert!(result.is_ok());

        let table = result.unwrap();
        assert_eq!(table.first_object_number, 0);
        assert_eq!(table.number_of_entries, 4);
        assert!(!table.entries.is_empty());
    }
}
