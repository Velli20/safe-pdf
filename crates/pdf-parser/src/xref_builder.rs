use std::collections::{BTreeMap, HashSet};

use pdf_object::{
    cross_reference_table::{CrossReferenceEntry, CrossReferenceTable},
    object_resolver::PassthroughResolver,
    object_variant::ObjectVariant,
};

use crate::{error::ParserError, parser::PdfParser};

impl PdfParser<'_> {
    /// Locates the cross-reference section via the trailing `startxref` marker,
    /// then follows the `/Prev` chain to produce a fully-merged [`CrossReferenceTable`].
    ///
    /// # Note on linearized PDFs
    ///
    /// This implementation always scans for the *last* `startxref` in the file.
    /// For linearized PDFs the last `startxref` points to the linearization
    /// parameter dictionary, not the primary xref. Full linearized-PDF support
    /// (using the *first* `startxref`) is not yet implemented.
    /// TODO: handle linearized PDFs by using the first `startxref` offset instead.
    pub fn build_xref_table(&mut self) -> Result<CrossReferenceTable, ParserError> {
        let xref_offset = self.find_startxref_offset()?;
        merge_xref_chain(self, xref_offset)
    }

    /// Scans backward through the input for the last `startxref` keyword and extracts
    /// the byte offset that follows it.
    fn find_startxref_offset(&mut self) -> Result<usize, ParserError> {
        const STARTXREF_KEYWORD: &[u8] = b"startxref";

        let startxref_pos = self
            .tokenizer
            .input
            .windows(STARTXREF_KEYWORD.len())
            .rposition(|window| window == STARTXREF_KEYWORD)
            .ok_or(ParserError::MissingStartXref)?;

        self.tokenizer.position = startxref_pos;
        self.read_keyword(b"startxref")?;
        self.read_number::<usize>(true)
            .map_err(|_| ParserError::MissingStartXref)
    }
}

fn parse_xref_section_at_offset(
    parser: &mut PdfParser,
    offset: usize,
) -> Result<CrossReferenceTable, ParserError> {
    let parsed = parser.parse_object_at(offset, &PassthroughResolver)?;

    match parsed {
        ObjectVariant::CrossReferenceTable(table) => Ok(table),
        ObjectVariant::IndirectObject(indirect) => match indirect.object {
            Some(ObjectVariant::Stream(ref stream)) => {
                crate::cross_reference_stream::parse_xref_stream(stream, &PassthroughResolver)
            }
            _ => Err(ParserError::InvalidXrefAtOffset { offset }),
        },
        ObjectVariant::Stream(ref stream) => {
            crate::cross_reference_stream::parse_xref_stream(stream, &PassthroughResolver)
        }
        _ => Err(ParserError::InvalidXrefAtOffset { offset }),
    }
}

fn recover_traditional_xref_offset(parser: &PdfParser, declared_offset: usize) -> Option<usize> {
    const XREF_KEYWORD: &[u8] = b"xref";
    const RECOVERY_WINDOW: usize = 1024;

    let input = parser.tokenizer.input;
    let search_start = declared_offset.saturating_sub(RECOVERY_WINDOW);
    let search_end = declared_offset.min(input.len());
    let haystack = input.get(search_start..search_end)?;

    haystack
        .windows(XREF_KEYWORD.len())
        .rposition(|window| window == XREF_KEYWORD)
        .and_then(|relative_position| search_start.checked_add(relative_position))
        .filter(|&candidate| candidate != declared_offset)
        .filter(|&candidate| {
            let starts_on_line_boundary =
                match candidate.checked_sub(1).and_then(|idx| input.get(idx)) {
                    Some(previous) => PdfParser::is_pdf_whitespace(*previous),
                    None => true,
                };
            let has_delimiter_after_keyword = match candidate
                .checked_add(XREF_KEYWORD.len())
                .and_then(|idx| input.get(idx))
            {
                Some(next) => PdfParser::is_pdf_whitespace(*next),
                None => true,
            };

            starts_on_line_boundary && has_delimiter_after_keyword
        })
}

fn normalize_traditional_xref_offsets(table: &mut CrossReferenceTable, offset_delta: usize) {
    if offset_delta == 0 {
        return;
    }

    for entry in table.entries.values_mut() {
        let pdf_object::cross_reference_table::CrossReferenceEntryType::Normal {
            byte_offset,
            generation_number,
        } = &entry.entry_type
        else {
            continue;
        };

        let adjusted_offset = byte_offset
            .checked_sub(offset_delta)
            .unwrap_or(*byte_offset);
        *entry = CrossReferenceEntry::new_normal(adjusted_offset, *generation_number);
    }
}

fn parse_xref_section_with_recovery(
    parser: &mut PdfParser,
    declared_offset: usize,
) -> Result<CrossReferenceTable, ParserError> {
    match parse_xref_section_at_offset(parser, declared_offset) {
        Ok(table) => Ok(table),
        Err(ParserError::InvalidXrefAtOffset { .. }) => {
            let recovered_offset = recover_traditional_xref_offset(parser, declared_offset).ok_or(
                ParserError::InvalidXrefAtOffset {
                    offset: declared_offset,
                },
            )?;
            let mut table = parse_xref_section_at_offset(parser, recovered_offset)?;
            normalize_traditional_xref_offsets(
                &mut table,
                declared_offset.saturating_sub(recovered_offset),
            );
            Ok(table)
        }
        Err(error) => Err(error),
    }
}

/// Follows the xref chain via `/Prev` entries and merges all cross-reference tables.
///
/// Handles incremental PDF updates where each update adds a new xref section
/// that references the previous one via the `/Prev` entry in the trailer.
/// Supports both traditional xref tables and xref streams at each step.
fn merge_xref_chain(
    parser: &mut PdfParser,
    start_offset: usize,
) -> Result<CrossReferenceTable, ParserError> {
    let mut entries: BTreeMap<usize, CrossReferenceEntry> = BTreeMap::new();
    let mut visited_offsets = HashSet::new();
    let mut current_offset = start_offset;
    let mut trailer = None;

    loop {
        // Prevent infinite loops from circular /Prev references.
        if !visited_offsets.insert(current_offset) {
            break;
        }

        let xref_table = parse_xref_section_with_recovery(parser, current_offset)?;

        // Merge entries: newer entries (already present) take precedence over older ones.
        for (obj_num, entry) in xref_table.entries {
            entries.entry(obj_num).or_insert(entry);
        }

        let prev_value = xref_table.trailer.dictionary.get("Prev").cloned();

        // Prefer the first (newest) trailer; fall back to one with /Root if needed.
        match trailer.as_ref() {
            None => {
                trailer = Some(xref_table.trailer);
            }
            Some(existing) if existing.dictionary.get("Root").is_none() => {
                if xref_table.trailer.dictionary.get("Root").is_some() {
                    trailer = Some(xref_table.trailer);
                }
            }
            _ => {}
        }

        if let Some(prev_value) = prev_value {
            current_offset = prev_value.try_number::<usize>(&PassthroughResolver)?;
        } else {
            break;
        }
    }

    let trailer = trailer.ok_or(ParserError::MissingStartXref)?;
    Ok(CrossReferenceTable::new(entries, trailer))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_object::object_resolver::PassthroughResolver;

    fn format_xref_entry(offset: usize, generation: u16, used: bool) -> String {
        let kind = if used { 'n' } else { 'f' };
        format!("{:010} {:05} {} \n", offset, generation, kind)
    }

    #[test]
    fn test_build_xref_table_simple() {
        let mut data = Vec::new();
        data.extend_from_slice(b"%PDF-1.7\n");

        let obj1_offset = data.len();
        data.extend_from_slice(b"1 0 obj\n<< /Type /Catalog >>\nendobj\n");

        let xref_offset = data.len();
        data.extend_from_slice(b"xref\n0 2\n");
        data.extend_from_slice(format_xref_entry(0, 65535, false).as_bytes());
        data.extend_from_slice(format_xref_entry(obj1_offset, 0, true).as_bytes());

        data.extend_from_slice(b"trailer\n<< /Size 2 /Root 1 0 R >>\n");
        data.extend_from_slice(b"startxref\n");
        data.extend_from_slice(format!("{}\n", xref_offset).as_bytes());
        data.extend_from_slice(b"%%EOF");

        let mut parser = PdfParser::from(data.as_slice());
        let result = parser.build_xref_table();

        assert!(
            result.is_ok(),
            "Should successfully build xref table: {:?}",
            result.err()
        );
        let table = result.unwrap();

        assert_eq!(table.entries.len(), 2, "Should have 2 entries");

        let entry1 = table.entries.get(&1).expect("Obj 1 should exist");
        assert_eq!(entry1.byte_offset(), Some(obj1_offset));

        let entry0 = table.entries.get(&0).expect("Obj 0 should exist");
        assert!(entry0.is_free(), "Obj 0 should be free");

        let size: i64 = table
            .trailer
            .dictionary
            .get("Size")
            .expect("Size expected")
            .try_number(&PassthroughResolver)
            .unwrap();
        assert_eq!(size, 2);
    }

    #[test]
    fn test_merge_xref_chain_incremental() {
        let mut data = Vec::new();
        data.extend_from_slice(b"%PDF-1.7\n");

        let obj1_v1_offset = data.len();
        data.extend_from_slice(b"1 0 obj\n(v1)\nendobj\n");
        let obj2_offset = data.len();
        data.extend_from_slice(b"2 0 obj\n(obj2)\nendobj\n");

        let xref1_offset = data.len();
        data.extend_from_slice(b"xref\n0 3\n");
        data.extend_from_slice(format_xref_entry(0, 65535, false).as_bytes());
        data.extend_from_slice(format_xref_entry(obj1_v1_offset, 0, true).as_bytes());
        data.extend_from_slice(format_xref_entry(obj2_offset, 0, true).as_bytes());

        data.extend_from_slice(b"trailer\n<< /Size 3 /Root 1 0 R >>\n");
        data.extend_from_slice(b"startxref\n");
        data.extend_from_slice(format!("{}\n", xref1_offset).as_bytes());
        data.extend_from_slice(b"%%EOF\n");

        let obj1_v2_offset = data.len();
        data.extend_from_slice(b"1 0 obj\n(v2)\nendobj\n");

        let xref2_offset = data.len();
        data.extend_from_slice(b"xref\n0 1\n");
        data.extend_from_slice(format_xref_entry(0, 65535, false).as_bytes());
        data.extend_from_slice(b"1 1\n");
        data.extend_from_slice(format_xref_entry(obj1_v2_offset, 0, true).as_bytes());

        data.extend_from_slice(b"trailer\n<< /Size 3 /Root 1 0 R /Prev ");
        data.extend_from_slice(format!("{}", xref1_offset).as_bytes());
        data.extend_from_slice(b" >>\n");
        data.extend_from_slice(b"startxref\n");
        data.extend_from_slice(format!("{}\n", xref2_offset).as_bytes());
        data.extend_from_slice(b"%%EOF");

        let mut parser = PdfParser::from(data.as_slice());
        let result = merge_xref_chain(&mut parser, xref2_offset);

        assert!(result.is_ok(), "Should merge xref chain");
        let table = result.unwrap();

        let entry1 = table.entries.get(&1).expect("Obj 1 missing");
        assert_eq!(
            entry1.byte_offset(),
            Some(obj1_v2_offset),
            "Obj 1 should point to v2"
        );

        let entry2 = table.entries.get(&2).expect("Obj 2 missing");
        assert_eq!(
            entry2.byte_offset(),
            Some(obj2_offset),
            "Obj 2 should be from v1"
        );
    }

    #[test]
    fn test_merge_xref_circular_protection() {
        let mut data = Vec::new();
        data.extend_from_slice(b"%PDF-1.7\n");

        while data.len() < 100 {
            data.push(b' ');
        }
        let xref1_offset = data.len();
        data.extend_from_slice(b"xref\n0 1\n");
        data.extend_from_slice(format_xref_entry(0, 65535, false).as_bytes());
        data.extend_from_slice(b"trailer\n<< /Prev 200 >>\n");
        data.extend_from_slice(b"startxref\n0\n%%EOF\n");

        while data.len() < 200 {
            data.push(b' ');
        }
        let xref2_offset = data.len();
        data.extend_from_slice(b"xref\n0 1\n");
        data.extend_from_slice(format_xref_entry(0, 65535, false).as_bytes());
        data.extend_from_slice(format!("trailer\n<< /Prev {} >>\n", xref1_offset).as_bytes());
        data.extend_from_slice(b"startxref\n0\n%%EOF");

        let mut parser = PdfParser::from(data.as_slice());
        let result = merge_xref_chain(&mut parser, xref2_offset);

        // Should break the loop rather than hang or crash.
        assert!(
            result.is_ok(),
            "Failed circular xref test: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_build_xref_table_recovers_stripped_header_offsets() {
        let mut data = Vec::new();
        data.extend_from_slice(b"%\xe2\xe3\xcf\xd3\n");

        let obj1_offset = data.len();
        data.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

        let obj2_offset = data.len();
        data.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n");

        let xref_offset = data.len();
        data.extend_from_slice(b"xref\n0 3\n");

        const STRIPPED_HEADER_DELTA: usize = 9;
        data.extend_from_slice(format_xref_entry(0, 65535, false).as_bytes());
        data.extend_from_slice(
            format_xref_entry(obj1_offset + STRIPPED_HEADER_DELTA, 0, true).as_bytes(),
        );
        data.extend_from_slice(
            format_xref_entry(obj2_offset + STRIPPED_HEADER_DELTA, 0, true).as_bytes(),
        );

        data.extend_from_slice(b"trailer\n<< /Size 3 /Root 1 0 R >>\n");
        data.extend_from_slice(b"startxref\n");
        data.extend_from_slice(format!("{}\n", xref_offset + STRIPPED_HEADER_DELTA).as_bytes());
        data.extend_from_slice(b"%%EOF");

        let mut parser = PdfParser::from(data.as_slice());
        let table = parser
            .build_xref_table()
            .expect("xref recovery should work");

        let entry1 = table.entries.get(&1).expect("obj 1 should exist");
        assert_eq!(entry1.byte_offset(), Some(obj1_offset));

        let entry2 = table.entries.get(&2).expect("obj 2 should exist");
        assert_eq!(entry2.byte_offset(), Some(obj2_offset));
    }
}
