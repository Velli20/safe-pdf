use std::collections::BTreeMap;

use pdf_object_reader::{
    cross_reference_table::{CrossReferenceEntryType, CrossReferenceStatus, CrossReferenceTable},
    object_resolver::ObjectResolver,
};
use pdf_tokenizer::PdfToken;
use thiserror::Error;

use crate::{error::ParserError, parser::PdfParser};

const XREF_KEYWORD: &[u8] = b"xref";

/// Represents an error that can occur while parsing a cross-reference table.
#[derive(Debug, PartialEq, Error)]
pub enum CrossReferenceTableError {
    #[error("Invalid cross-reference status character: '{0}'")]
    InvalidCrossReferenceStatus(char),
    #[error("Missing status in cross-reference entry")]
    MissingStatus,
}

impl PdfParser<'_> {
    pub fn parse_cross_reference_table(
        &mut self,
        objects: &dyn ObjectResolver,
    ) -> Result<CrossReferenceTable, ParserError> {
        self.read_keyword(XREF_KEYWORD)?;
        let entries = self.parse_cross_reference_subsections()?;
        let trailer = self.parse_trailer(objects)?;

        Ok(CrossReferenceTable::new(entries, trailer))
    }

    /// Parses the contiguous subsections that follow an `xref` keyword.
    pub(crate) fn parse_cross_reference_subsections(
        &mut self,
    ) -> Result<BTreeMap<usize, CrossReferenceEntryType>, ParserError> {
        let mut entries = BTreeMap::new();

        loop {
            self.skip_whitespace_and_comments();
            if !matches!(self.tokenizer.peek(), Some(PdfToken::Number(_))) {
                break;
            }

            let (start_object_number, subsection_entries) =
                self.parse_cross_reference_subsection()?;
            entries.extend(normalize_xref_subsection_entries(
                start_object_number,
                subsection_entries,
            ));
        }

        Ok(entries)
    }

    /// Parses one xref subsection header and its declared number of entries.
    fn parse_cross_reference_subsection(
        &mut self,
    ) -> Result<(usize, Vec<CrossReferenceEntryType>), ParserError> {
        let start_object_number = self.read_number::<usize>(true)?;
        self.skip_whitespace_and_comments();
        let entry_count = self.read_number::<usize>(true)?;
        let mut entries = Vec::with_capacity(entry_count);

        for _ in 0..entry_count {
            entries.push(self.parse_cross_reference_entry()?);
        }

        Ok((start_object_number, entries))
    }

    /// Parses a traditional xref row into its corresponding entry.
    pub(crate) fn parse_cross_reference_entry(
        &mut self,
    ) -> Result<CrossReferenceEntryType, ParserError> {
        self.skip_whitespace_and_comments();
        let field1 = self.read_number::<usize>(true)?;
        self.skip_whitespace_and_comments();
        let field2 = self.read_number::<usize>(true)?;
        self.skip_whitespace_and_comments();

        let status_byte = match self.tokenizer.read() {
            Some(PdfToken::Alphabetic(byte)) => byte,
            _ => return Err(CrossReferenceTableError::MissingStatus.into()),
        };

        let status = CrossReferenceStatus::from_byte(status_byte).ok_or_else(|| {
            CrossReferenceTableError::InvalidCrossReferenceStatus(char::from(status_byte))
        })?;

        Ok(match status {
            CrossReferenceStatus::Normal => CrossReferenceEntryType::new_normal(field1, field2),
            CrossReferenceStatus::Free | CrossReferenceStatus::Old => {
                CrossReferenceEntryType::new_free(field1, field2)
            }
        })
    }
}

/// Normalizes an xref subsection's object numbers, including the malformed
/// leading free-object-zero pattern encountered in some PDFs.
fn normalize_xref_subsection_entries(
    start_object_number: usize,
    entries: Vec<CrossReferenceEntryType>,
) -> Vec<(usize, CrossReferenceEntryType)> {
    let has_leading_free_object_zero = start_object_number > 0
        && entries
            .first()
            .is_some_and(is_malformed_leading_free_object_zero);

    entries
        .into_iter()
        .enumerate()
        .map(|(index, entry)| {
            let object_number = if has_leading_free_object_zero {
                if index == 0 {
                    0
                } else {
                    start_object_number.saturating_add(index.saturating_sub(1))
                }
            } else {
                start_object_number.saturating_add(index)
            };
            (object_number, entry)
        })
        .collect()
}

/// Returns whether an entry is the object-zero free entry incorrectly included
/// at the start of a non-zero xref subsection.
fn is_malformed_leading_free_object_zero(entry: &CrossReferenceEntryType) -> bool {
    matches!(
        entry,
        CrossReferenceEntryType::Free {
            next_free_object: 0,
            generation_number: 65_535,
        }
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
mod tests {
    use pdf_object_reader::object_resolver::PassthroughResolver;

    use super::*;

    #[test]
    fn test_parse_valid_xref_section() {
        let data = b"xref\n0 2\n0000000000 65535 f\n0000000017 00000 n\ntrailer\n<< /Size 2 >>\nstartxref\n0\n";
        let mut parser = PdfParser::from(data.as_slice());
        let table = parser
            .parse_cross_reference_table(&PassthroughResolver)
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

        let result = parser.parse_cross_reference_table(&PassthroughResolver);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_empty_xref_section() {
        let data = b"xref\n0 0\ntrailer\n<< /Size 0 >>\nstartxref\n0\n";
        let mut parser = PdfParser::from(data.as_slice());

        let table = parser
            .parse_cross_reference_table(&PassthroughResolver)
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
            .parse_cross_reference_table(&PassthroughResolver)
            .unwrap();
        assert_eq!(table.entries.len(), 4);
        assert!(table.entries.contains_key(&0));
        assert!(table.entries.contains_key(&1));
        assert!(table.entries.contains_key(&4));
        assert!(table.entries.contains_key(&5));
    }

    #[test]
    fn test_parse_xref_section_with_comment_between_entries() {
        let data = b"xref\n0 3\n0000000000 65535 f\n0000000017 00000 n\n% comment between rows\n0000000081 00000 n\ntrailer\n<< /Size 3 >>\nstartxref\n0\n";
        let mut parser = PdfParser::from(data.as_slice());

        let table = parser
            .parse_cross_reference_table(&PassthroughResolver)
            .unwrap();

        assert_eq!(table.entries.len(), 3);
        assert!(table.entries[&0].is_free());
        assert_eq!(table.entries[&1].byte_offset(), Some(17));
        assert_eq!(table.entries[&2].byte_offset(), Some(81));
    }

    #[test]
    fn test_parse_old_xref_entry_as_free() {
        let data = b"xref\n1 1\n0000000017 00000 o\ntrailer\n<< /Size 2 >>\nstartxref\n0\n";
        let mut parser = PdfParser::from(data.as_slice());

        let table = parser
            .parse_cross_reference_table(&PassthroughResolver)
            .unwrap();

        assert!(table.entries[&1].is_free());
    }

    #[test]
    fn test_parse_xref_entry_rejects_invalid_status() {
        let data = b"xref\n0 1\n0000000000 65535 x\n";
        let mut parser = PdfParser::from(data.as_slice());

        let error = parser
            .parse_cross_reference_table(&PassthroughResolver)
            .unwrap_err();

        assert_eq!(
            error,
            ParserError::CrossReferenceTableError(
                CrossReferenceTableError::InvalidCrossReferenceStatus('x')
            )
        );
    }

    #[test]
    fn test_parse_xref_entry_requires_status() {
        let data = b"xref\n0 1\n0000000000 65535";
        let mut parser = PdfParser::from(data.as_slice());

        let error = parser
            .parse_cross_reference_table(&PassthroughResolver)
            .unwrap_err();

        assert_eq!(
            error,
            ParserError::CrossReferenceTableError(CrossReferenceTableError::MissingStatus)
        );
    }

    #[test]
    fn test_normalize_xref_subsection_entries_uses_declared_range() {
        let entries = vec![
            CrossReferenceEntryType::new_normal(17, 0),
            CrossReferenceEntryType::new_normal(81, 0),
        ];

        let normalized = normalize_xref_subsection_entries(4, entries);

        assert_eq!(normalized[0].0, 4);
        assert_eq!(normalized[1].0, 5);
    }

    #[test]
    fn test_normalize_xref_subsection_entries_handles_leading_free_object_zero() {
        let entries = vec![
            CrossReferenceEntryType::new_free(0, 65_535),
            CrossReferenceEntryType::new_normal(17, 0),
            CrossReferenceEntryType::new_normal(81, 0),
        ];

        let normalized = normalize_xref_subsection_entries(4, entries);

        assert_eq!(normalized[0].0, 0);
        assert_eq!(normalized[1].0, 4);
        assert_eq!(normalized[2].0, 5);
    }
}
