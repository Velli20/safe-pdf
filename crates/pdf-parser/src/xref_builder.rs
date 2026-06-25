use std::collections::{BTreeMap, HashSet};

use pdf_object::{
    cross_reference_table::{CrossReferenceEntry, CrossReferenceEntryType, CrossReferenceTable},
    object_resolver::PassthroughResolver,
    object_variant::ObjectVariant,
    stream::StreamObject,
    trailer::Trailer,
};

use crate::{error::ParserError, parser::PdfParser};

const STARTXREF_KEYWORD: &[u8] = b"startxref";
const XREF_KEYWORD: &[u8] = b"xref";
const XREF_RECOVERY_WINDOW: usize = 1024;
const MALFORMED_XREF_SEARCH_WINDOW: usize = 4096;
const MIN_TRADITIONAL_REPAIR_RADIUS: usize = 64;
const XREF_STREAM_REPAIR_RADIUS: usize = 1024;

#[derive(Clone, Copy, Eq, PartialEq)]
/// Identifies whether a parsed cross-reference section came from a traditional
/// table or from a cross-reference stream.
enum XrefSectionKind {
    Traditional,
    Stream,
}

/// A parsed cross-reference section together with its source kind.
struct ParsedXrefSection {
    kind: XrefSectionKind,
    table: CrossReferenceTable,
}

#[derive(Clone, Copy)]
/// Repair policy used to normalize object offsets after parsing a section.
struct OffsetRepairPolicy {
    adjustment: usize,
    search_radius: usize,
    fall_back_to_latest_match: bool,
}

impl OffsetRepairPolicy {
    /// Builds the repair policy for a traditional xref table.
    fn traditional(adjustment: usize) -> Self {
        Self {
            adjustment,
            search_radius: adjustment.max(MIN_TRADITIONAL_REPAIR_RADIUS),
            fall_back_to_latest_match: true,
        }
    }

    /// Builds the repair policy for a cross-reference stream.
    const fn stream() -> Self {
        Self {
            adjustment: 0,
            search_radius: XREF_STREAM_REPAIR_RADIUS,
            fall_back_to_latest_match: false,
        }
    }
}

/// Coordinates locating, parsing, repairing, and validating xref sections.
struct XrefBuilder<'parser, 'input> {
    parser: &'parser mut PdfParser<'input>,
}

impl<'parser, 'input> XrefBuilder<'parser, 'input> {
    /// Creates a builder around the parser that owns the input cursor.
    fn new(parser: &'parser mut PdfParser<'input>) -> Self {
        Self { parser }
    }

    /// Tries all discovered `startxref` offsets until one yields a valid table.
    fn build(&mut self) -> Result<CrossReferenceTable, ParserError> {
        let mut last_error = None;

        for offset in self.startxref_offsets()? {
            match self
                // Parse the section chain first, then validate the merged result.
                .merge_chain(offset)
                .and_then(|table| self.validate(table, offset))
            {
                Ok(table) => return Ok(table),
                Err(error) => last_error = Some(error),
            }
        }

        Err(last_error.unwrap_or(ParserError::MissingStartXref))
    }

    /// Scans the input for `startxref` markers and reads the offsets they point to.
    fn startxref_offsets(&mut self) -> Result<Vec<usize>, ParserError> {
        let positions: Vec<_> = self
            .parser
            .tokenizer
            .input
            .windows(STARTXREF_KEYWORD.len())
            .enumerate()
            .filter_map(|(position, window)| (window == STARTXREF_KEYWORD).then_some(position))
            .collect();

        let mut offsets = Vec::with_capacity(positions.len());
        for position in positions.into_iter().rev() {
            // Work from the most recent marker backward so newer revisions win.
            let offset = self.at_position(position, |builder| {
                builder.parser.read_keyword(STARTXREF_KEYWORD)?;
                builder.parser.read_number::<usize>(true)
            });

            if let Ok(offset) = offset {
                offsets.push(offset);
            }
        }

        if offsets.is_empty() {
            Err(ParserError::MissingStartXref)
        } else {
            Ok(offsets)
        }
    }

    /// Follows the `/Prev` chain and merges the visible entries into one table.
    fn merge_chain(&mut self, start_offset: usize) -> Result<CrossReferenceTable, ParserError> {
        let mut entries = BTreeMap::new();
        let mut visited_offsets = HashSet::new();
        let mut current_offset = start_offset;
        let mut trailer = None;

        while visited_offsets.insert(current_offset) {
            let table = self.parse_section_with_recovery(current_offset)?;
            let auxiliary_offset = table.trailer.dictionary.get("XRefStm").cloned();
            let previous_offset = table.trailer.dictionary.get("Prev").cloned();

            // Keep the first entry we see for each object number.
            Self::merge_entries(&mut entries, table.entries);

            if let Some(offset) = auxiliary_offset {
                let offset = offset.try_number::<usize>(&PassthroughResolver)?;
                if visited_offsets.insert(offset) {
                    let auxiliary_table = self.parse_section_with_recovery(offset)?;
                    // Traditional tables are authoritative in hybrid revisions.
                    Self::merge_entries(&mut entries, auxiliary_table.entries);
                }
            }

            // Prefer the newest trailer that exposes a root catalog.
            Self::prefer_newer_trailer(&mut trailer, table.trailer);

            let Some(previous_offset) = previous_offset else {
                break;
            };
            current_offset = previous_offset.try_number::<usize>(&PassthroughResolver)?;
        }

        let trailer = trailer.ok_or(ParserError::MissingStartXref)?;
        Ok(CrossReferenceTable::new(entries, trailer))
    }

    /// Merges a parsed subsection into the accumulated object map.
    fn merge_entries(
        entries: &mut BTreeMap<usize, CrossReferenceEntry>,
        section_entries: BTreeMap<usize, CrossReferenceEntry>,
    ) {
        for (object_number, entry) in section_entries {
            let _ = entries.entry(object_number).or_insert(entry);
        }
    }

    /// Replaces the trailer if the candidate carries better root metadata.
    fn prefer_newer_trailer(preferred: &mut Option<Trailer>, candidate: Trailer) {
        let should_replace = preferred
            .as_ref()
            .is_some_and(|trailer| trailer.dictionary.get("Root").is_none())
            && candidate.dictionary.get("Root").is_some();

        if preferred.is_none() || should_replace {
            *preferred = Some(candidate);
        }
    }

    /// Parses a section and falls back to nearby recovery strategies on failure.
    fn parse_section_with_recovery(
        &mut self,
        declared_offset: usize,
    ) -> Result<CrossReferenceTable, ParserError> {
        match self.parse_section_at(declared_offset) {
            Ok(section) => Ok(self.repair_section(section, 0)),
            Err(original_error) => {
                if let Some(recovered_offset) = self.recover_section_offset(declared_offset) {
                    let section = self.parse_section_at(recovered_offset)?;
                    return Ok(
                        self.repair_section(section, declared_offset.abs_diff(recovered_offset))
                    );
                }

                self.parse_malformed_rows_at(declared_offset)
                    .map(|section| self.repair_section(section, 0))
                    .map_err(|_| original_error)
            }
        }
    }

    /// Parses a cross-reference section at the exact byte offset.
    fn parse_section_at(&mut self, offset: usize) -> Result<ParsedXrefSection, ParserError> {
        let object = self.parser.parse_object_at(offset, &PassthroughResolver)?;

        match object {
            ObjectVariant::CrossReferenceTable(table) => Ok(ParsedXrefSection {
                kind: XrefSectionKind::Traditional,
                table,
            }),
            ObjectVariant::IndirectObject(indirect) => match indirect.object {
                Some(ObjectVariant::Stream(stream)) => self.parse_stream_section(stream),
                _ => Err(ParserError::InvalidXrefAtOffset { offset }),
            },
            ObjectVariant::Stream(stream) => self.parse_stream_section(stream),
            _ => Err(ParserError::InvalidXrefAtOffset { offset }),
        }
    }

    /// Wraps a parsed xref stream in the common section representation.
    fn parse_stream_section(&self, stream: StreamObject) -> Result<ParsedXrefSection, ParserError> {
        Ok(ParsedXrefSection {
            kind: XrefSectionKind::Stream,
            table: crate::cross_reference_stream::parse_xref_stream(&stream, &PassthroughResolver)?,
        })
    }

    /// Attempts to parse malformed xref rows when the declared offset is wrong.
    fn parse_malformed_rows_at(&mut self, offset: usize) -> Result<ParsedXrefSection, ParserError> {
        if offset > self.parser.tokenizer.input.len() {
            let recovered_offset = self
                .recover_malformed_offset(offset)
                .ok_or(ParserError::InvalidXrefAtOffset { offset })?;
            return self.parse_malformed_rows_at(recovered_offset);
        }

        self.at_position(offset, |builder| {
            builder.parser.skip_whitespace_and_comments();
            builder.skip_optional_xref_keyword();
            builder.parse_malformed_rows()
        })
    }

    /// Consumes an `xref` keyword when malformed row recovery starts at it.
    fn skip_optional_xref_keyword(&mut self) {
        let mark = self.parser.tokenizer.position;
        if self.parser.read_keyword(XREF_KEYWORD).is_err() {
            self.parser.tokenizer.position = mark;
        }
    }

    /// Parses malformed rows while preserving the cursor on failure.
    fn parse_malformed_rows(&mut self) -> Result<ParsedXrefSection, ParserError> {
        let mark = self.parser.tokenizer.position;
        match self.parse_malformed_subsections() {
            Ok(section) => Ok(section),
            Err(_) => {
                self.parser.tokenizer.position = mark;
                self.parse_malformed_entries()
            }
        }
    }

    /// Parses malformed xref subsection blocks using the declared object ranges.
    fn parse_malformed_subsections(&mut self) -> Result<ParsedXrefSection, ParserError> {
        let entries = self.parser.parse_cross_reference_subsections()?;
        self.finish_malformed_section(entries)
    }

    /// Parses a malformed xref table as a flat sequence of entries.
    fn parse_malformed_entries(&mut self) -> Result<ParsedXrefSection, ParserError> {
        let mut entries = BTreeMap::new();
        let mut object_number = 0usize;

        loop {
            self.parser.skip_whitespace_and_comments();
            if !matches!(
                self.parser.tokenizer.peek(),
                Some(pdf_tokenizer::PdfToken::Number(_))
            ) {
                break;
            }

            let entry = self.try_parse_malformed_entry()?;
            let _ = entries.insert(object_number, entry);
            object_number = object_number.saturating_add(1);
        }

        self.finish_malformed_section(entries)
    }

    /// Finishes malformed parsing by reading the trailer and assembling the table.
    fn finish_malformed_section(
        &mut self,
        entries: BTreeMap<usize, CrossReferenceEntry>,
    ) -> Result<ParsedXrefSection, ParserError> {
        if entries.is_empty() {
            return Err(self.invalid_xref_error());
        }

        let trailer = self.parser.parse_trailer(&PassthroughResolver)?;
        Ok(ParsedXrefSection {
            kind: XrefSectionKind::Traditional,
            table: CrossReferenceTable::new(entries, trailer),
        })
    }

    /// Probes a malformed row without consuming input on failure.
    fn try_parse_malformed_entry(&mut self) -> Result<CrossReferenceEntry, ParserError> {
        let mark = self.parser.tokenizer.position;
        let result = self.parser.parse_cross_reference_entry();
        if result.is_err() {
            self.parser.tokenizer.position = mark;
        }
        result
    }

    /// Searches near the declared offset for a better xref section location.
    fn recover_section_offset(&mut self, declared_offset: usize) -> Option<usize> {
        let input = self.parser.tokenizer.input;
        let search_start = declared_offset.saturating_sub(XREF_RECOVERY_WINDOW);
        let search_end = declared_offset
            .saturating_add(XREF_RECOVERY_WINDOW)
            .min(input.len());
        let mut candidates = HashSet::new();

        if let Some(haystack) = input.get(search_start..search_end) {
            for (relative_position, window) in haystack.windows(XREF_KEYWORD.len()).enumerate() {
                if window == XREF_KEYWORD
                    && let Some(candidate) = search_start.checked_add(relative_position)
                    && candidate != declared_offset
                {
                    let _ = candidates.insert(candidate);
                }
            }

            for candidate in search_start..search_end {
                if candidate != declared_offset
                    && self.parser.looks_like_indirect_object_header_at(candidate)
                {
                    let _ = candidates.insert(candidate);
                }
            }
        }

        if candidates.is_empty() || declared_offset > input.len() {
            for (candidate, window) in input.windows(XREF_KEYWORD.len()).enumerate() {
                if window != XREF_KEYWORD || candidate == declared_offset {
                    continue;
                }

                let previous_is_regular = candidate
                    .checked_sub(1)
                    .and_then(|index| input.get(index))
                    .is_some_and(|byte| PdfParser::is_pdf_regular_character(*byte));
                if !previous_is_regular {
                    let _ = candidates.insert(candidate);
                }
            }
        }

        self.closest_valid_candidate(candidates, declared_offset)
    }

    /// Searches for a better offset when a malformed table appears shifted.
    fn recover_malformed_offset(&mut self, declared_offset: usize) -> Option<usize> {
        let input = self.parser.tokenizer.input;
        let search_start = input.len().saturating_sub(MALFORMED_XREF_SEARCH_WINDOW);
        let mut candidates = Vec::new();

        for candidate in search_start..input.len() {
            let Some(byte) = input.get(candidate).copied() else {
                continue;
            };
            if !byte.is_ascii_digit() {
                continue;
            }

            let starts_on_line_boundary = candidate == 0
                || candidate
                    .checked_sub(1)
                    .and_then(|index| input.get(index))
                    .is_some_and(|byte| PdfParser::is_pdf_whitespace(*byte));
            if starts_on_line_boundary && !self.looks_like_malformed_entry(candidate) {
                candidates.push(candidate);
            }
        }

        candidates.sort_by(|left, right| {
            left.abs_diff(declared_offset)
                .cmp(&right.abs_diff(declared_offset))
                .then_with(|| right.cmp(left))
        });

        let mut best_candidate = None;
        for candidate in candidates {
            // Re-parse each candidate and prefer the one with the strongest trailer.
            let result = self.at_position(candidate, |builder| {
                builder.parser.skip_whitespace_and_comments();
                builder.parse_malformed_rows()
            });
            let Ok(section) = result else {
                continue;
            };

            let entry_count = section.table.entries.len();
            if entry_count == 0 {
                continue;
            }
            let has_valid_root = matches!(
                section.table.trailer.dictionary.get("Root"),
                Some(ObjectVariant::Reference(object_number))
                    if section.table.entries.contains_key(object_number)
            );

            match best_candidate {
                Some((_, best_entry_count, best_has_valid_root))
                    if (best_has_valid_root && !has_valid_root)
                        || (best_has_valid_root == has_valid_root
                            && best_entry_count >= entry_count) => {}
                _ => best_candidate = Some((candidate, entry_count, has_valid_root)),
            }
        }

        best_candidate.map(|(candidate, _, _)| candidate)
    }

    /// Chooses the nearest candidate that parses as a valid xref section.
    fn closest_valid_candidate(
        &mut self,
        candidates: HashSet<usize>,
        declared_offset: usize,
    ) -> Option<usize> {
        let mut candidates: Vec<_> = candidates.into_iter().collect();
        candidates.sort_by(|left, right| {
            left.abs_diff(declared_offset)
                .cmp(&right.abs_diff(declared_offset))
                .then_with(|| right.cmp(left))
        });

        candidates
            .into_iter()
            .find(|candidate| self.parse_section_at(*candidate).is_ok())
    }

    /// Checks whether a byte offset looks like the start of a malformed entry.
    fn looks_like_malformed_entry(&mut self, offset: usize) -> bool {
        self.at_position(offset, |builder| {
            builder.parser.skip_whitespace();
            builder.parser.parse_cross_reference_entry().is_ok()
                && builder
                    .parser
                    .tokenizer
                    .data()
                    .first()
                    .copied()
                    .is_some_and(PdfParser::is_pdf_whitespace)
        })
    }

    /// Repairs all normal xref offsets according to the parsed section kind.
    fn repair_section(
        &self,
        mut section: ParsedXrefSection,
        offset_adjustment: usize,
    ) -> CrossReferenceTable {
        let policy = match section.kind {
            XrefSectionKind::Traditional => OffsetRepairPolicy::traditional(offset_adjustment),
            XrefSectionKind::Stream => OffsetRepairPolicy::stream(),
        };
        self.repair_offsets(&mut section.table, policy);
        section.table
    }

    /// Normalizes byte offsets by re-checking them against the input stream.
    fn repair_offsets(&self, table: &mut CrossReferenceTable, policy: OffsetRepairPolicy) {
        let mut invalid_entries = Vec::new();

        for (&object_number, entry) in &mut table.entries {
            let CrossReferenceEntryType::Normal {
                byte_offset,
                generation_number,
            } = &entry.entry_type
            else {
                continue;
            };

            // Free entries are stable; only normal entries may need repair.
            if *byte_offset == 0 {
                continue;
            }

            let declared_offset = *byte_offset;
            let generation_number = *generation_number;
            let recovered_offset = self
                .is_indirect_object_at(declared_offset, object_number, generation_number)
                .then_some(declared_offset)
                .or_else(|| {
                    declared_offset
                        .checked_sub(policy.adjustment)
                        .filter(|offset| {
                            self.is_indirect_object_at(*offset, object_number, generation_number)
                        })
                })
                .or_else(|| {
                    self.nearby_indirect_object_offset(
                        object_number,
                        generation_number,
                        declared_offset,
                        policy.search_radius,
                    )
                })
                .or_else(|| {
                    policy
                        .fall_back_to_latest_match
                        // Older tables may need the last matching occurrence instead.
                        .then(|| {
                            self.latest_indirect_object_offset(object_number, generation_number)
                        })
                        .flatten()
                });

            match recovered_offset {
                Some(offset) => *entry = CrossReferenceEntry::new_normal(offset, generation_number),
                None => invalid_entries.push(object_number),
            }
        }

        for object_number in invalid_entries {
            let _ = table.entries.remove(&object_number);
        }
    }

    /// Searches the surrounding range for an indirect object header match.
    fn nearby_indirect_object_offset(
        &self,
        object_number: usize,
        generation_number: usize,
        declared_offset: usize,
        search_radius: usize,
    ) -> Option<usize> {
        let search_start = declared_offset.saturating_sub(search_radius);
        let search_end = declared_offset
            .saturating_add(search_radius)
            .saturating_add(1)
            .min(self.parser.tokenizer.input.len());

        (search_start..search_end)
            .filter(|offset| self.is_indirect_object_at(*offset, object_number, generation_number))
            .min_by(|left, right| {
                left.abs_diff(declared_offset)
                    .cmp(&right.abs_diff(declared_offset))
                    .then_with(|| right.cmp(left))
            })
    }

    /// Falls back to the latest matching object header in the input.
    fn latest_indirect_object_offset(
        &self,
        object_number: usize,
        generation_number: usize,
    ) -> Option<usize> {
        (0..self.parser.tokenizer.input.len())
            .rev()
            .find(|offset| self.is_indirect_object_at(*offset, object_number, generation_number))
    }

    /// Checks whether the parser sees the requested indirect object header there.
    fn is_indirect_object_at(
        &self,
        offset: usize,
        object_number: usize,
        generation_number: usize,
    ) -> bool {
        self.parser
            .matches_indirect_object_header_at(offset, object_number, generation_number)
    }

    /// Ensures the merged table only contains entries that match the input.
    fn validate(
        &self,
        table: CrossReferenceTable,
        xref_offset: usize,
    ) -> Result<CrossReferenceTable, ParserError> {
        let is_valid = table.entries.iter().all(|(&object_number, entry)| {
            let CrossReferenceEntryType::Normal {
                byte_offset,
                generation_number,
            } = &entry.entry_type
            else {
                return true;
            };

            *byte_offset == 0
                || self.is_indirect_object_at(*byte_offset, object_number, *generation_number)
        });

        if is_valid {
            Ok(table)
        } else {
            Err(ParserError::InvalidXrefAtOffset {
                offset: xref_offset,
            })
        }
    }

    /// Builds the parser error for invalid xref content at the current cursor.
    fn invalid_xref_error(&self) -> ParserError {
        ParserError::InvalidXrefAtOffset {
            offset: self.parser.tokenizer.position,
        }
    }

    /// Temporarily moves the cursor to a position, then restores it afterward.
    fn at_position<T>(&mut self, position: usize, operation: impl FnOnce(&mut Self) -> T) -> T {
        let mark = self.parser.tokenizer.position;
        self.parser.tokenizer.position = position;
        let result = operation(self);
        self.parser.tokenizer.position = mark;
        result
    }
}

impl PdfParser<'_> {
    /// Locates a cross-reference section via `startxref` markers, then follows
    /// the `/Prev` chain to produce a fully-merged [`CrossReferenceTable`].
    pub fn build_xref_table(&mut self) -> Result<CrossReferenceTable, ParserError> {
        // Keep the parser-facing API small; the builder owns the actual workflow.
        XrefBuilder::new(self).build()
    }
}

#[cfg(test)]
mod tests {
    use pdf_object::cross_reference_table::CrossReferenceEntry;

    use super::*;

    /// Formats a single textual xref row for the parser tests.
    fn format_xref_entry(offset: usize, generation: usize, status: char) -> Vec<u8> {
        format!("{offset:010} {generation:05} {status} \n").into_bytes()
    }

    /// Verifies malformed entry probing rolls the cursor back on failure.
    #[test]
    fn malformed_entry_probe_restores_position_after_failure() {
        let mut parser = PdfParser::from(b"not-an-xref entry".as_slice());
        parser.tokenizer.position = 3;
        let mut builder = XrefBuilder::new(&mut parser);

        let result = builder.try_parse_malformed_entry();

        assert!(result.is_err());
        assert_eq!(builder.parser.tokenizer.position, 3);
    }

    /// Verifies malformed entry probing accepts a valid xref row.
    #[test]
    fn malformed_entry_probe_parses_valid_entry() {
        let mut data = format_xref_entry(12, 34, 'n');
        data.extend_from_slice(b"trailer");
        let mut parser = PdfParser::from(data.as_slice());
        let mut builder = XrefBuilder::new(&mut parser);

        let entry = builder.try_parse_malformed_entry().unwrap();

        assert_eq!(entry, CrossReferenceEntry::new_normal(12, 34));
        assert!(matches!(
            builder.parser.tokenizer.data().first().copied(),
            Some(b' ') | Some(b'\n')
        ));
    }

    /// Verifies a malformed entry must be terminated by whitespace.
    #[test]
    fn malformed_entry_detection_requires_terminator() {
        let data = format_xref_entry(12, 34, 'n');
        let mut parser = PdfParser::from(data.as_slice());
        let mut builder = XrefBuilder::new(&mut parser);

        assert!(builder.looks_like_malformed_entry(0));

        let truncated = b"0000000012 00034 n".to_vec();
        let mut parser = PdfParser::from(truncated.as_slice());
        let mut builder = XrefBuilder::new(&mut parser);

        assert!(!builder.looks_like_malformed_entry(0));
        assert_eq!(builder.parser.tokenizer.position, 0);
    }

    /// Verifies scanning for `startxref` preserves the parser's cursor.
    #[test]
    fn startxref_scan_restores_parser_position() {
        let data = b"startxref\n12\n";
        let mut parser = PdfParser::from(data.as_slice());
        parser.tokenizer.position = 4;
        let mut builder = XrefBuilder::new(&mut parser);

        let offsets = builder.startxref_offsets().unwrap();

        assert_eq!(offsets, vec![12]);
        assert_eq!(builder.parser.tokenizer.position, 4);
    }
}
