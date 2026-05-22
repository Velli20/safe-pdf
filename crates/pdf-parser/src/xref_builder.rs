use std::collections::{BTreeMap, HashSet};

use pdf_object::{
    cross_reference_table::{CrossReferenceEntry, CrossReferenceEntryType, CrossReferenceTable},
    object_resolver::PassthroughResolver,
    object_variant::ObjectVariant,
};

use crate::{error::ParserError, parser::PdfParser};

impl PdfParser<'_> {
    /// Locates a cross-reference section via `startxref` markers, then follows
    /// the `/Prev` chain to produce a fully-merged [`CrossReferenceTable`].
    pub fn build_xref_table(&mut self) -> Result<CrossReferenceTable, ParserError> {
        let xref_offsets = self.find_startxref_offsets()?;
        let mut last_error = None;

        for xref_offset in xref_offsets {
            match merge_xref_chain(self, xref_offset)
                .and_then(|table| validate_xref_table(self, table, xref_offset))
            {
                Ok(table) => return Ok(table),
                Err(error) => last_error = Some(error),
            }
        }

        Err(last_error.unwrap_or(ParserError::MissingStartXref))
    }

    /// Scans backward through the input for `startxref` keywords and extracts
    /// the byte offsets that follow them, newest first.
    fn find_startxref_offsets(&mut self) -> Result<Vec<usize>, ParserError> {
        const STARTXREF_KEYWORD: &[u8] = b"startxref";

        let positions: Vec<usize> = self
            .tokenizer
            .input
            .windows(STARTXREF_KEYWORD.len())
            .enumerate()
            .filter_map(|(position, window)| {
                if window == STARTXREF_KEYWORD {
                    Some(position)
                } else {
                    None
                }
            })
            .collect();

        if positions.is_empty() {
            return Err(ParserError::MissingStartXref);
        }

        let mut offsets = Vec::with_capacity(positions.len());
        let original_position = self.tokenizer.position;

        for startxref_pos in positions.into_iter().rev() {
            self.tokenizer.position = startxref_pos;
            if self.read_keyword(b"startxref").is_ok()
                && let Ok(offset) = self.read_number::<usize>(true)
            {
                offsets.push(offset);
            }
        }

        self.tokenizer.position = original_position;

        if offsets.is_empty() {
            return Err(ParserError::MissingStartXref);
        }

        Ok(offsets)
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
    let search_end = declared_offset
        .saturating_add(RECOVERY_WINDOW)
        .min(input.len());
    let haystack = input.get(search_start..search_end)?;

    haystack
        .windows(XREF_KEYWORD.len())
        .enumerate()
        .filter_map(|(relative_position, window)| {
            (window == XREF_KEYWORD)
                .then(|| search_start.checked_add(relative_position))
                .flatten()
        })
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
        .min_by_key(|candidate| candidate.abs_diff(declared_offset))
}

fn matches_indirect_object_header(
    input: &[u8],
    offset: usize,
    object_number: usize,
    generation_number: usize,
) -> bool {
    let object_number = object_number.to_string();
    let generation_number = generation_number.to_string();
    let object_number = object_number.as_bytes();
    let generation_number = generation_number.as_bytes();

    let Some(data) = input.get(offset..) else {
        return false;
    };
    if !data.starts_with(object_number) {
        return false;
    }

    let mut position = object_number.len();
    position = match skip_required_whitespace(data, position) {
        Some(position) => position,
        None => return false,
    };

    let Some(remaining) = data.get(position..) else {
        return false;
    };
    if !remaining.starts_with(generation_number) {
        return false;
    }

    position = position.saturating_add(generation_number.len());
    position = match skip_required_whitespace(data, position) {
        Some(position) => position,
        None => return false,
    };

    let Some(keyword) = data.get(position..position.saturating_add(3)) else {
        return false;
    };
    if keyword != b"obj" {
        return false;
    }

    match data.get(position.saturating_add(3)).copied() {
        Some(next) => PdfParser::is_pdf_delimiter(next),
        None => true,
    }
}

fn skip_required_whitespace(input: &[u8], start: usize) -> Option<usize> {
    let mut position = start;
    let mut consumed = false;

    while let Some(byte) = input.get(position).copied() {
        if !PdfParser::is_pdf_whitespace(byte) {
            break;
        }
        consumed = true;
        position = position.saturating_add(1);
    }

    consumed.then_some(position)
}

fn find_nearby_indirect_object_offset(
    input: &[u8],
    object_number: usize,
    generation_number: usize,
    declared_offset: usize,
    search_radius: usize,
) -> Option<usize> {
    let search_start = declared_offset.saturating_sub(search_radius);
    let search_end = declared_offset
        .saturating_add(search_radius)
        .saturating_add(1)
        .min(input.len());
    let mut best_candidate = None;

    for candidate in search_start..search_end {
        if !matches_indirect_object_header(input, candidate, object_number, generation_number) {
            continue;
        }

        let distance = candidate.abs_diff(declared_offset);
        match best_candidate {
            Some((best_offset, best_distance))
                if best_distance < distance
                    || (best_distance == distance && best_offset <= candidate) => {}
            _ => best_candidate = Some((candidate, distance)),
        }
    }

    best_candidate.map(|(candidate, _)| candidate)
}

fn repair_traditional_xref_offsets(
    parser: &PdfParser,
    table: &mut CrossReferenceTable,
    offset_delta: usize,
) {
    const MIN_SEARCH_RADIUS: usize = 64;

    if offset_delta == 0 {
        return;
    }

    let input = parser.tokenizer.input;
    let search_radius = offset_delta.max(MIN_SEARCH_RADIUS);

    for (&object_number, entry) in &mut table.entries {
        let CrossReferenceEntryType::Normal {
            byte_offset,
            generation_number,
        } = &entry.entry_type
        else {
            continue;
        };

        if *byte_offset == 0
            || matches_indirect_object_header(
                input,
                *byte_offset,
                object_number,
                *generation_number,
            )
        {
            continue;
        }

        if let Some(adjusted_offset) = byte_offset.checked_sub(offset_delta)
            && matches_indirect_object_header(
                input,
                adjusted_offset,
                object_number,
                *generation_number,
            )
        {
            *entry = CrossReferenceEntry::new_normal(adjusted_offset, *generation_number);
            continue;
        }

        if let Some(recovered_offset) = find_nearby_indirect_object_offset(
            input,
            object_number,
            *generation_number,
            *byte_offset,
            search_radius,
        ) {
            *entry = CrossReferenceEntry::new_normal(recovered_offset, *generation_number);
        }
    }
}

fn validate_xref_table(
    parser: &PdfParser,
    table: CrossReferenceTable,
    xref_offset: usize,
) -> Result<CrossReferenceTable, ParserError> {
    let input = parser.tokenizer.input;

    for (&object_number, entry) in &table.entries {
        let CrossReferenceEntryType::Normal {
            byte_offset,
            generation_number,
        } = &entry.entry_type
        else {
            continue;
        };

        if *byte_offset == 0 {
            continue;
        }

        if !matches_indirect_object_header(input, *byte_offset, object_number, *generation_number) {
            return Err(ParserError::InvalidXrefAtOffset {
                offset: xref_offset,
            });
        }
    }

    Ok(table)
}

fn parse_xref_section_with_recovery(
    parser: &mut PdfParser,
    declared_offset: usize,
) -> Result<CrossReferenceTable, ParserError> {
    match parse_xref_section_at_offset(parser, declared_offset) {
        Ok(table) => Ok(table),
        Err(original_error) => {
            let recovered_offset =
                recover_traditional_xref_offset(parser, declared_offset).ok_or(original_error)?;
            let mut table = parse_xref_section_at_offset(parser, recovered_offset)?;
            repair_traditional_xref_offsets(
                parser,
                &mut table,
                declared_offset.abs_diff(recovered_offset),
            );
            Ok(table)
        }
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
        let auxiliary_xref_stream_offset = xref_table
            .trailer
            .dictionary
            .get("XRefStm")
            .cloned();

        // Merge entries: newer entries (already present) take precedence over older ones.
        for (obj_num, entry) in xref_table.entries {
            entries.entry(obj_num).or_insert(entry);
        }

        if let Some(xref_stream_offset) = auxiliary_xref_stream_offset {
            let xref_stream_offset = xref_stream_offset.try_number::<usize>(&PassthroughResolver)?;

            if visited_offsets.insert(xref_stream_offset) {
                let auxiliary_xref_table =
                    parse_xref_section_with_recovery(parser, xref_stream_offset)?;

                // Hybrid-reference files use /XRefStm to supplement the traditional xref
                // section with entries such as compressed objects. Keep the traditional
                // section authoritative for duplicates within the same revision.
                for (obj_num, entry) in auxiliary_xref_table.entries {
                    entries.entry(obj_num).or_insert(entry);
                }
            }
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
    use std::collections::BTreeMap;

    use super::*;
    use pdf_object::object_resolver::PassthroughResolver;

    fn format_xref_entry(offset: usize, generation: u16, used: bool) -> String {
        let kind = if used { 'n' } else { 'f' };
        format!("{:010} {:05} {} \n", offset, generation, kind)
    }

    fn build_issue139_like_pdf() -> (Vec<u8>, BTreeMap<usize, usize>) {
        let mut data = Vec::new();
        data.extend_from_slice(b"%PDF-1.4\n");

        let obj6_offset = data.len();
        data.extend_from_slice(b"6 0 obj\n<<\n /Type /Catalog\n /Pages 5 0 R\n>>\nendobj\n\n");

        let obj1_offset = data.len();
        data.extend_from_slice(
            b"1 0 obj\n<<\n /Type /Page\n /Parent 5 0 R\n /MediaBox [ 0 0 612 792 ]\n /Resources 3 0 R\n /Contents 2 0 R\n>>\nendobj\n\n",
        );

        let obj4_offset = data.len();
        data.extend_from_slice(
            b"4 0 obj\n<<\n /Type /Font\n /Subtype /Type1\n /Name /F1\n /BaseFont/Helvetica\n>>\nendobj\n\n",
        );

        let obj2_offset = data.len();
        data.extend_from_slice(
            b"2 0 obj\n<<\n /Length 53\n>>\nstream\ntoString\nendstream\nendobj\n\n",
        );

        let obj5_offset = data.len();
        data.extend_from_slice(
            b"5 0 obj\n<<\n /Type /Pages\n /Kids [ 1 0 R ]\n /Count 1\n>>\nendobj\n\n",
        );

        let obj3_offset = data.len();
        data.extend_from_slice(
            b"3 0 obj\n<<\n /ProcSet[/PDF/Text]\n /Font <</F1 4 0 R >>\n>>\nendobj\n\n",
        );

        let stream_payload_offset = data
            .windows(b"toString".len())
            .position(|window| window == b"toString")
            .expect("stream payload should exist");

        let xref_offset = data.len();
        data.extend_from_slice(b"xref\n0 7\n");
        data.extend_from_slice(format_xref_entry(0, 65535, false).as_bytes());
        data.extend_from_slice(
            format_xref_entry(obj1_offset.saturating_sub(8), 0, true).as_bytes(),
        );
        data.extend_from_slice(format_xref_entry(stream_payload_offset, 0, true).as_bytes());
        data.extend_from_slice(
            format_xref_entry(obj3_offset.saturating_add(4), 0, true).as_bytes(),
        );
        data.extend_from_slice(
            format_xref_entry(obj4_offset.saturating_sub(3), 0, true).as_bytes(),
        );
        data.extend_from_slice(
            format_xref_entry(obj5_offset.saturating_add(9), 0, true).as_bytes(),
        );
        data.extend_from_slice(format_xref_entry(obj6_offset, 0, true).as_bytes());

        data.extend_from_slice(b"trailer\n<< /Size 7 /Root 6 0 R >>\n");
        data.extend_from_slice(b"startxref\n");
        data.extend_from_slice(format!("{}\n", xref_offset.saturating_add(36)).as_bytes());
        data.extend_from_slice(b"%%EOF");

        (
            data,
            BTreeMap::from([
                (1, obj1_offset),
                (2, obj2_offset),
                (3, obj3_offset),
                (4, obj4_offset),
                (5, obj5_offset),
                (6, obj6_offset),
            ]),
        )
    }

    fn build_hybrid_xref_pdf() -> Vec<u8> {
        fn push_xref_stream_entry(data: &mut Vec<u8>, entry_type: u8, field2: u16, field3: u8) {
            data.push(entry_type);
            data.extend_from_slice(&field2.to_be_bytes());
            data.push(field3);
        }

        let mut data = Vec::new();
        data.extend_from_slice(b"%PDF-1.7\n");

        let obj1_offset = data.len();
        data.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

        let object_2 = b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>";
        let object_3 = b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>";
        let object_2_offset = 0usize;
        let object_3_offset = object_2.len().saturating_add(1);
        let object_stream_header =
            format!("2 {object_2_offset} 3 {object_3_offset} ").into_bytes();
        let first = object_stream_header.len();
        let mut object_stream_data = object_stream_header;
        object_stream_data.extend_from_slice(object_2);
        object_stream_data.push(b' ');
        object_stream_data.extend_from_slice(object_3);

        let obj4_offset = data.len();
        data.extend_from_slice(
            format!(
                "4 0 obj\n<< /Type /ObjStm /N 2 /First {first} /Length {} >>\nstream\n",
                object_stream_data.len()
            )
            .as_bytes(),
        );
        data.extend_from_slice(&object_stream_data);
        data.extend_from_slice(b"\nendstream\nendobj\n");

        let obj5_offset = data.len();
        let mut xref_stream_data = Vec::new();
        push_xref_stream_entry(&mut xref_stream_data, 0, 0, u8::MAX);
        push_xref_stream_entry(&mut xref_stream_data, 1, obj1_offset as u16, 0);
        push_xref_stream_entry(&mut xref_stream_data, 2, 4, 0);
        push_xref_stream_entry(&mut xref_stream_data, 2, 4, 1);
        push_xref_stream_entry(&mut xref_stream_data, 1, obj4_offset as u16, 0);
        push_xref_stream_entry(&mut xref_stream_data, 1, obj5_offset as u16, 0);
        data.extend_from_slice(
            format!(
                "5 0 obj\n<< /Type /XRef /Size 6 /W [1 2 1] /Index [0 6] /Length {} >>\nstream\n",
                xref_stream_data.len()
            )
            .as_bytes(),
        );
        data.extend_from_slice(&xref_stream_data);
        data.extend_from_slice(b"\nendstream\nendobj\n");

        let xref_offset = data.len();
        data.extend_from_slice(b"xref\n0 2\n");
        data.extend_from_slice(format_xref_entry(0, 65535, false).as_bytes());
        data.extend_from_slice(format_xref_entry(obj1_offset, 0, true).as_bytes());
        data.extend_from_slice(b"4 2\n");
        data.extend_from_slice(format_xref_entry(obj4_offset, 0, true).as_bytes());
        data.extend_from_slice(format_xref_entry(obj5_offset, 0, true).as_bytes());
        data.extend_from_slice(
            format!(
                "trailer\n<< /Size 6 /Root 1 0 R /XRefStm {obj5_offset} >>\nstartxref\n{xref_offset}\n%%EOF"
            )
            .as_bytes(),
        );

        data
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
    fn test_build_xref_table_falls_back_from_invalid_newer_xref() {
        let mut data = Vec::new();
        data.extend_from_slice(b"%PDF-1.7\n");

        let obj1_offset = data.len();
        data.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

        let obj2_offset = data.len();
        data.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n");

        let xref1_offset = data.len();
        data.extend_from_slice(b"xref\n0 3\n");
        data.extend_from_slice(format_xref_entry(0, 65535, false).as_bytes());
        data.extend_from_slice(format_xref_entry(obj1_offset, 0, true).as_bytes());
        data.extend_from_slice(format_xref_entry(obj2_offset, 0, true).as_bytes());
        data.extend_from_slice(b"trailer\n<< /Size 3 /Root 1 0 R >>\n");
        data.extend_from_slice(b"startxref\n");
        data.extend_from_slice(format!("{}\n", xref1_offset).as_bytes());
        data.extend_from_slice(b"%%EOF\n");

        let invalid_obj2_offset = obj2_offset.saturating_add(2);
        let xref2_offset = data.len();
        data.extend_from_slice(b"xref\n0 3\n");
        data.extend_from_slice(format_xref_entry(0, 65535, false).as_bytes());
        data.extend_from_slice(format_xref_entry(obj1_offset, 0, true).as_bytes());
        data.extend_from_slice(format_xref_entry(invalid_obj2_offset, 0, true).as_bytes());
        data.extend_from_slice(b"trailer\n<< /Size 3 /Root 1 0 R /Prev ");
        data.extend_from_slice(format!("{}", xref1_offset).as_bytes());
        data.extend_from_slice(b" >>\n");
        data.extend_from_slice(b"startxref\n");
        data.extend_from_slice(format!("{}\n", xref2_offset).as_bytes());
        data.extend_from_slice(b"%%EOF");

        let mut parser = PdfParser::from(data.as_slice());
        let table = parser
            .build_xref_table()
            .expect("invalid newer xref should fall back to older valid xref");

        let entry2 = table.entries.get(&2).expect("obj 2 should exist");
        assert_eq!(entry2.byte_offset(), Some(obj2_offset));
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

    #[test]
    fn test_build_xref_table_recovers_startxref_inside_endstream() {
        let mut data = Vec::new();
        data.extend_from_slice(b"%PDF-1.7\n");

        let obj1_offset = data.len();
        data.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

        let obj2_offset = data.len();
        data.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n");

        let obj3_offset = data.len();
        data.extend_from_slice(b"3 0 obj\n<< /Length 0 >>\nstream\n\nendstream\nendobj\n");

        let bad_startxref_offset = data
            .windows(b"endstream".len())
            .position(|window| window == b"endstream")
            .expect("test fixture should contain endstream")
            .saturating_add(1);

        let xref_offset = data.len();
        data.extend_from_slice(b"xref\n0 4\n");
        data.extend_from_slice(format_xref_entry(0, 65535, false).as_bytes());
        data.extend_from_slice(format_xref_entry(obj1_offset, 0, true).as_bytes());
        data.extend_from_slice(
            format_xref_entry(obj2_offset.saturating_sub(3), 0, true).as_bytes(),
        );
        data.extend_from_slice(
            format_xref_entry(obj3_offset.saturating_sub(2), 0, true).as_bytes(),
        );

        data.extend_from_slice(b"trailer\n<< /Size 4 /Root 1 0 R >>\n");
        data.extend_from_slice(b"startxref\n");
        data.extend_from_slice(format!("{bad_startxref_offset}\n").as_bytes());
        data.extend_from_slice(b"%%EOF");

        let mut parser = PdfParser::from(data.as_slice());
        let table = parser
            .build_xref_table()
            .expect("xref recovery should find the later traditional table");

        let entry1 = table.entries.get(&1).expect("obj 1 should exist");
        assert_eq!(entry1.byte_offset(), Some(obj1_offset));

        let entry2 = table.entries.get(&2).expect("obj 2 should exist");
        assert_eq!(entry2.byte_offset(), Some(obj2_offset));

        let entry3 = table.entries.get(&3).expect("obj 3 should exist");
        assert_eq!(entry3.byte_offset(), Some(obj3_offset));
        assert!(bad_startxref_offset < xref_offset);
    }

    #[test]
    fn test_build_xref_table_repairs_issue139_offsets() {
        let (data, expected_offsets) = build_issue139_like_pdf();
        let mut parser = PdfParser::from(data.as_slice());
        let table = parser
            .build_xref_table()
            .expect("xref recovery should repair nearby object offsets");

        for object_number in 1..=6 {
            let entry = table
                .entries
                .get(&object_number)
                .expect("expected normal object entry");
            let expected_offset = expected_offsets
                .get(&object_number)
                .copied()
                .expect("expected known object offset");
            let byte_offset = entry
                .byte_offset()
                .expect("entry should have a byte offset");
            assert_eq!(
                byte_offset, expected_offset,
                "object {object_number} should recover its true offset"
            );
            assert!(
                matches_indirect_object_header(data.as_slice(), byte_offset, object_number, 0),
                "object {object_number} should point to its indirect object header, got offset {byte_offset}"
            );
        }
    }

    #[test]
    fn test_build_xref_table_merges_hybrid_xref_stream_entries() {
        let data = build_hybrid_xref_pdf();
        let mut parser = PdfParser::from(data.as_slice());
        let table = parser
            .build_xref_table()
            .expect("hybrid xref tables should merge /XRefStm entries");

        let pages_entry = table.entries.get(&2).expect("obj 2 should exist");
        match &pages_entry.entry_type {
            CrossReferenceEntryType::Compressed {
                object_stream_number,
                index_within_stream,
            } => {
                assert_eq!(*object_stream_number, 4);
                assert_eq!(*index_within_stream, 0);
            }
            other => panic!("expected compressed xref entry for obj 2, got {other:?}"),
        }

        let page_entry = table.entries.get(&3).expect("obj 3 should exist");
        match &page_entry.entry_type {
            CrossReferenceEntryType::Compressed {
                object_stream_number,
                index_within_stream,
            } => {
                assert_eq!(*object_stream_number, 4);
                assert_eq!(*index_within_stream, 1);
            }
            other => panic!("expected compressed xref entry for obj 3, got {other:?}"),
        }
    }
}
