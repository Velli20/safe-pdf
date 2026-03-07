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
        let xref_offset = find_startxref_offset(self.tokenizer.input)?;
        merge_xref_chain(self, xref_offset)
    }
}

/// Scans backward through `input` for the last `startxref` keyword and extracts
/// the byte offset that follows it.
fn find_startxref_offset(input: &[u8]) -> Result<usize, ParserError> {
    const STARTXREF_KEYWORD: &[u8] = b"startxref";

    let startxref_pos = input
        .windows(STARTXREF_KEYWORD.len())
        .rposition(|window| window == STARTXREF_KEYWORD)
        .ok_or(ParserError::MissingStartXref)?;

    let offset_start = startxref_pos.saturating_add(STARTXREF_KEYWORD.len());
    let remaining = input
        .get(offset_start..)
        .ok_or(ParserError::MissingStartXref)?;

    // Skip whitespace, then read the digit run.
    let digits_start = remaining
        .iter()
        .position(|b| b.is_ascii_digit())
        .ok_or(ParserError::MissingStartXref)?;

    // `digits_start` came from `position()` so `get(digits_start..)` always succeeds.
    let digit_slice = remaining
        .get(digits_start..)
        .ok_or(ParserError::MissingStartXref)?;
    let digit_count = digit_slice
        .iter()
        .position(|b| !b.is_ascii_digit())
        .unwrap_or(digit_slice.len());
    let digits_end = digits_start.saturating_add(digit_count);

    let xref_offset: usize = std::str::from_utf8(
        remaining
            .get(digits_start..digits_end)
            .ok_or(ParserError::MissingStartXref)?,
    )
    .map_err(|_| ParserError::MissingStartXref)?
    .parse()
    .map_err(|_| ParserError::MissingStartXref)?;

    Ok(xref_offset)
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

        let parsed = parser.parse_object_at(current_offset, &PassthroughResolver)?;

        let xref_table = match parsed {
            ObjectVariant::CrossReferenceTable(table) => table,
            ObjectVariant::IndirectObject(indirect) => match indirect.object {
                Some(ObjectVariant::Stream(ref stream)) => {
                    crate::cross_reference_stream::parse_xref_stream(stream, &PassthroughResolver)?
                }
                _ => {
                    return Err(ParserError::InvalidXrefAtOffset {
                        offset: current_offset,
                    });
                }
            },
            ObjectVariant::Stream(ref stream) => {
                crate::cross_reference_stream::parse_xref_stream(stream, &PassthroughResolver)?
            }
            _ => {
                return Err(ParserError::InvalidXrefAtOffset {
                    offset: current_offset,
                });
            }
        };

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
}
