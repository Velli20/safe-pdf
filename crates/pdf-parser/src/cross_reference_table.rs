use pdf_object::{
    cross_reference_table::{CrossReferenceEntry, CrossReferenceStatus, CrossReferenceTable},
    object_resolver::ObjectResolver,
};
use pdf_tokenizer::PdfToken;
use thiserror::Error;

use crate::{
    error::ParserError,
    parser::PdfParser,
    traits::{CrossReferenceTableParser, TrailerParser},
};

/// Represents an error that can occur while parsing a cross-reference table.
#[derive(Debug, PartialEq, Error)]
pub enum CrossReferenceTableError {
    #[error("Invalid cross-reference status character: '{0}'")]
    InvalidCrossReferenceStatus(char),
    #[error("Missing status in cross-reference entry")]
    MissingStatus,
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
    /// ^          ^     ^
    /// |          |     └─ usage indicator: `n` (in use) or `f` (free)
    /// |          └──── 5-digit generation number (0-padded)
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
    fn parse_cross_reference_table(
        &mut self,
        objects: &dyn ObjectResolver,
    ) -> Result<CrossReferenceTable, Self::ErrorType> {
        const XREF_KEYWORD: &[u8] = b"xref";

        self.read_keyword(XREF_KEYWORD)?;
        self.skip_whitespace();

        let mut entries = std::collections::BTreeMap::new();

        // Parse sections while we see a number (start of a new section).
        while matches!(self.tokenizer.peek(), Some(PdfToken::Number(_))) {
            let start_object_number = self.read_number::<usize>(true)?;
            let count = self.read_number::<usize>(true)?;

            for i in 0..count {
                let byte_offset = self.read_number::<usize>(true)?;
                let generation_number = self.read_number::<usize>(true)?;

                let status_byte = match self.tokenizer.read() {
                    Some(PdfToken::Alphabetic(b)) => b,
                    _ => return Err(CrossReferenceTableError::MissingStatus.into()),
                };

                let status = CrossReferenceStatus::from_byte(status_byte).ok_or_else(|| {
                    CrossReferenceTableError::InvalidCrossReferenceStatus(char::from(status_byte))
                })?;

                entries.insert(
                    start_object_number.saturating_add(i),
                    CrossReferenceEntry::new(byte_offset, generation_number, status),
                );

                self.skip_whitespace();
            }
        }

        let trailer = self.parse_trailer(objects)?;

        Ok(CrossReferenceTable::new(entries, trailer))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
mod tests {
    use pdf_object::object_resolver::UnimplementedResolver;

    use super::*;

    #[test]
    fn test_parse_valid_xref_section() {
        let data = b"xref\n0 2\n0000000000 65535 f\n0000000017 00000 n\ntrailer\n<< /Size 2 >>\nstartxref\n0\n";
        let mut parser = PdfParser::from(data.as_slice());
        let table = parser
            .parse_cross_reference_table(&UnimplementedResolver)
            .unwrap();
        assert_eq!(table.entries.len(), 2);

        let entry0 = &table.entries[&0];
        assert_eq!(entry0.byte_offset, 0);
        assert_eq!(entry0.generation_number, 65535);
        assert_eq!(entry0.status, CrossReferenceStatus::Free);

        let entry1 = &table.entries[&1];
        assert_eq!(entry1.byte_offset, 17);
        assert_eq!(entry1.generation_number, 0);
        assert_eq!(entry1.status, CrossReferenceStatus::Normal);
    }

    #[test]
    fn test_parse_missing_entries() {
        let data = b"xref\n0 2\n0000000000 65535 f\n";
        let mut parser = PdfParser::from(data.as_slice());

        let result = parser.parse_cross_reference_table(&UnimplementedResolver);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_empty_xref_section() {
        let data = b"xref\n0 0\ntrailer\n<< /Size 0 >>\nstartxref\n0\n";
        let mut parser = PdfParser::from(data.as_slice());

        let table = parser
            .parse_cross_reference_table(&UnimplementedResolver)
            .unwrap();
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
        trailer\n<< /Size 6 >>\nstartxref\n0\n";

        let mut parser = PdfParser::from(data.as_slice());
        let table = parser
            .parse_cross_reference_table(&UnimplementedResolver)
            .unwrap();
        assert_eq!(table.entries.len(), 4);
        assert!(table.entries.contains_key(&0));
        assert!(table.entries.contains_key(&1));
        assert!(table.entries.contains_key(&4));
        assert!(table.entries.contains_key(&5));
    }
}
