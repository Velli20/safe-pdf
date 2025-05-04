use pdf_object::cross_reference_table::{
    CrossReferenceEntry, CrossReferenceStatus, CrossReferenceTable,
};
use pdf_tokenizer::PdfToken;

use crate::{ParseObject, PdfParser, error::ParserError};

/// Represents an error that can occur while parsing a cross-reference table.
#[derive(Debug, PartialEq)]
pub enum CrossReferenceTableError {
    /// Indicates that the status character in a cross-reference entry is invalid.
    InvalidCrossReferenceStatus(u8),
    /// Indicates that the entry count in a cross-reference table is missing.
    MissingTableEntryCount,
    /// Indicates that the object number in a cross-reference entry is missing.
    MissingObjectNumber,
    /// Indicates that the generation number in a cross-reference entry is missing.
    MissingGenerationNumber,
    /// Indicates that the status in a cross-reference entry is missing.
    MissingStatus,
    /// Indicates that number of entries read does not match the expected count.
    MissigTableEntries(usize, usize),
}

impl ParseObject<CrossReferenceTable> for PdfParser<'_> {
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
    fn parse(&mut self) -> Result<CrossReferenceTable, ParserError> {
        const XREF_KEYWORD: &[u8] = b"xref";

        // Expect the `xref` keyword.
        self.read_keyword(XREF_KEYWORD)?;

        let mut total_number_of_entries = 0;
        let mut first_object_number = None;

        let mut entries = Vec::new();
        loop {
            // Read the first object number.
            if let Some(PdfToken::Number(_)) = self.tokenizer.peek()? {
                let first_object_number_in_section =
                    self.read_number::<i32>(ParserError::InvalidNumber)?;
                if first_object_number.is_none() {
                    first_object_number = Some(first_object_number_in_section);
                }
            }

            // Read the number of objects.
            let number_of_objects =
                self.read_number::<u32>(CrossReferenceTableError::MissingTableEntryCount)?;

            // Read the entries.
            for _ in 0..number_of_objects {
                total_number_of_entries += 1;

                // Read the object number.
                let object_number =
                    self.read_number::<u32>(CrossReferenceTableError::MissingObjectNumber)?;

                // Read the generation number.
                let generation_number =
                    self.read_number::<u16>(CrossReferenceTableError::MissingGenerationNumber)?;

                // Read the status.
                if let Some(PdfToken::Alphabetic(e)) = self.tokenizer.read() {
                    let status = CrossReferenceStatus::from_byte(e).ok_or(ParserError::from(
                        CrossReferenceTableError::InvalidCrossReferenceStatus(e),
                    ))?;
                    entries.push(CrossReferenceEntry::new(
                        object_number,
                        generation_number,
                        status,
                    ));
                } else {
                    return Err(ParserError::CrossReferenceTableError(
                        CrossReferenceTableError::MissingStatus,
                    ));
                }
                self.skip_whitespace();
            }

            // If the next token is not a number, we are done reading entries.
            if !matches!(self.tokenizer.peek()?, Some(PdfToken::Number(_))) {
                if entries.len() != total_number_of_entries as usize {
                    println!(
                        "Expected {} entries, but found {}",
                        total_number_of_entries,
                        entries.len()
                    );
                    return Err(ParserError::CrossReferenceTableError(
                        CrossReferenceTableError::MissigTableEntries(
                            total_number_of_entries as usize,
                            entries.len(),
                        ),
                    ));
                }
                break;
            }
        }

        // Create a new cross-reference table.
        Ok(CrossReferenceTable::new(
            first_object_number.unwrap_or(0_i32) as u32,
            total_number_of_entries as u32,
            entries,
        ))
    }
}

impl std::fmt::Display for CrossReferenceTableError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CrossReferenceTableError::InvalidCrossReferenceStatus(status) => {
                write!(
                    f,
                    "Invalid cross-reference status charachter: '{}'",
                    String::from_utf8_lossy(&[*status])
                )
            }
            CrossReferenceTableError::MissingObjectNumber => {
                write!(f, "Missing object number in cross-reference entry")
            }
            CrossReferenceTableError::MissingGenerationNumber => {
                write!(f, "Missing generation number in cross-reference entry")
            }
            CrossReferenceTableError::MissingStatus => {
                write!(f, "Missing status in cross-reference entry")
            }
            CrossReferenceTableError::MissingTableEntryCount => {
                write!(f, "Missing entry count in cross-reference table")
            }
            CrossReferenceTableError::MissigTableEntries(expected, actual) => {
                write!(
                    f,
                    "Missing one or more table entries. Expected {} entries, but found {}",
                    expected, actual
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_xref_section() {
        let data = b"xref\n0 2\n0000000000 65535 f\n0000000017 00000 n\n";
        let mut parser = PdfParser::from(data.as_slice());

        let result: Result<CrossReferenceTable, ParserError> = parser.parse();
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

        let result: Result<CrossReferenceTable, ParserError> = parser.parse();
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_empty_xref_section() {
        let data = b"xref\n0 0\n";
        let mut parser = PdfParser::from(data.as_slice());

        let result: Result<CrossReferenceTable, ParserError> = parser.parse();
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

        let result: Result<CrossReferenceTable, ParserError> = parser.parse();
        assert!(result.is_ok());

        let table = result.unwrap();
        assert_eq!(table.first_object_number, 0);
        assert_eq!(table.number_of_entries, 4);
        assert!(!table.entries.is_empty());
    }
}
