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
                let field1 = self.read_number::<usize>(true)?;
                let field2 = self.read_number::<usize>(true)?;

                let status_byte = match self.tokenizer.read() {
                    Some(PdfToken::Alphabetic(b)) => b,
                    _ => return Err(CrossReferenceTableError::MissingStatus.into()),
                };

                let status = CrossReferenceStatus::from_byte(status_byte).ok_or_else(|| {
                    CrossReferenceTableError::InvalidCrossReferenceStatus(char::from(status_byte))
                })?;

                let entry = match status {
                    CrossReferenceStatus::Normal => {
                        CrossReferenceEntry::new_normal(field1, field2)
                    }
                    CrossReferenceStatus::Free | CrossReferenceStatus::Old => {
                        CrossReferenceEntry::new_free(field1, field2)
                    }
                };

                entries.insert(start_object_number.saturating_add(i), entry);

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
        assert!(entry0.is_free());

        let entry1 = &table.entries[&1];
        assert!(entry1.is_normal());
        assert_eq!(entry1.byte_offset(), Some(17));
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
