//! Recovery and repair for malformed or absent cross-reference data.
//!
//! Normal cross-reference discovery and exact parsing belong to
//! [`crate::xref_builder`]. This module is the explicit fallback boundary for PDFs
//! whose declared xref locations, rows, or object offsets are unusable. Keeping these
//! heuristics separate prevents tolerant scanning from becoming an implicit part of
//! the normal parser.

use std::collections::{BTreeMap, HashSet};

use pdf_object_reader::{
    cross_reference_table::{CrossReferenceEntryType, CrossReferenceTable},
    object_resolver::PassthroughResolver,
    object_variant::ObjectVariant,
};

use crate::{
    error::ParserError,
    linear_object_scanner::LinearObjectScanner,
    parser::PdfParser,
    xref_builder::{ParsedXrefSection, XrefSectionKind, parse_section_at},
};

const XREF_KEYWORD: &[u8] = b"xref";
const XREF_RECOVERY_WINDOW: usize = 1024;
const MALFORMED_XREF_SEARCH_WINDOW: usize = 4096;
const MIN_TRADITIONAL_REPAIR_RADIUS: usize = 64;
const XREF_STREAM_REPAIR_RADIUS: usize = 1024;

/// Controls how aggressively parsed object offsets are repaired.
#[derive(Clone, Copy)]
struct OffsetRepairPolicy {
    adjustment: usize,
    search_radius: usize,
    fall_back_to_latest_match: bool,
}

impl OffsetRepairPolicy {
    /// Uses section displacement as a hint and permits whole-file fallback.
    fn traditional(adjustment: usize) -> Self {
        Self {
            adjustment,
            search_radius: adjustment.max(MIN_TRADITIONAL_REPAIR_RADIUS),
            fall_back_to_latest_match: true,
        }
    }

    /// Restricts xref-stream repair to nearby matching object declarations.
    const fn stream() -> Self {
        Self {
            adjustment: 0,
            search_radius: XREF_STREAM_REPAIR_RADIUS,
            fall_back_to_latest_match: false,
        }
    }
}

/// Owns an independent cursor used exclusively for xref recovery probes.
pub(crate) struct XrefRecovery<'input> {
    parser: PdfParser<'input>,
}

impl<'input> XrefRecovery<'input> {
    /// Creates a recovery context over the parser's complete immutable input.
    pub(crate) fn new(parser: &PdfParser<'input>) -> Self {
        Self {
            parser: PdfParser::from(parser.tokenizer.input),
        }
    }

    /// Creates an independent recovery probe at an absolute byte offset.
    fn at_offset(&self, offset: usize) -> Result<Self, ParserError> {
        Ok(Self {
            parser: self.parser.at_offset(offset)?,
        })
    }

    /// Reconstructs an object index when no numeric `startxref` is available.
    pub(crate) fn rebuild_without_xref(&self) -> Result<CrossReferenceTable, ParserError> {
        self.validate(LinearObjectScanner::new(&self.parser).scan()?, 0)
    }

    /// Parses exactly first, then tries nearby sections and malformed row layouts.
    pub(crate) fn parse_section_with_recovery(
        &mut self,
        declared_offset: usize,
    ) -> Result<CrossReferenceTable, ParserError> {
        match parse_section_at(&self.parser, declared_offset) {
            Ok(section) => Ok(self.repair_section(section, 0)),
            Err(original_error) => {
                if let Some(recovered_offset) = self.recover_section_offset(declared_offset) {
                    let section = parse_section_at(&self.parser, recovered_offset)?;
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

    /// Attempts to parse malformed xref rows at or near a declared offset.
    fn parse_malformed_rows_at(&mut self, offset: usize) -> Result<ParsedXrefSection, ParserError> {
        if offset > self.parser.tokenizer.input.len() {
            let recovered_offset = self
                .recover_malformed_offset(offset)
                .ok_or(ParserError::InvalidXrefAtOffset { offset })?;
            return self.parse_malformed_rows_at(recovered_offset);
        }

        let mut recovery = self
            .at_offset(offset)
            .map_err(|_| ParserError::InvalidXrefAtOffset { offset })?;
        recovery.parser.skip_whitespace_and_comments();
        recovery.skip_optional_xref_keyword();
        recovery.parse_malformed_rows()
    }

    /// Consumes `xref` when malformed-row recovery happens to begin at it.
    fn skip_optional_xref_keyword(&mut self) {
        let mark = self.parser.tokenizer.position;
        if self.parser.read_keyword(XREF_KEYWORD).is_err() {
            self.parser.tokenizer.position = mark;
        }
    }

    /// Tries subsection-aware parsing before interpreting a flat row sequence.
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

    /// Parses malformed subsection blocks using their declared object ranges.
    fn parse_malformed_subsections(&mut self) -> Result<ParsedXrefSection, ParserError> {
        let entries = self.parser.parse_cross_reference_subsections()?;
        self.finish_malformed_section(entries)
    }

    /// Parses a malformed table represented as an unnumbered flat row sequence.
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

    /// Reads the trailer and wraps recovered rows as a traditional section.
    fn finish_malformed_section(
        &mut self,
        entries: BTreeMap<usize, CrossReferenceEntryType>,
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

    /// Probes one malformed row and restores the cursor if parsing fails.
    fn try_parse_malformed_entry(&mut self) -> Result<CrossReferenceEntryType, ParserError> {
        let mark = self.parser.tokenizer.position;
        let result = self.parser.parse_cross_reference_entry();
        if result.is_err() {
            self.parser.tokenizer.position = mark;
        }
        result
    }

    /// Searches around a declared position for a strictly parseable xref section.
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

                if self.parser.is_token_start_at(candidate) {
                    let _ = candidates.insert(candidate);
                }
            }
        }

        self.closest_valid_candidate(candidates, declared_offset)
    }

    /// Searches the file tail for malformed rows when the declared offset is shifted.
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
            let result = self.at_offset(candidate).and_then(|mut recovery| {
                recovery.parser.skip_whitespace_and_comments();
                recovery.parse_malformed_rows()
            });
            let Ok(section) = result else {
                continue;
            };

            let entry_count = section.table.entries.len();
            if entry_count == 0 {
                continue;
            }
            let has_valid_root = matches!(
                section.table.trailer.dictionary.get(b"Root"),
                Some(ObjectVariant::Reference(object_number))
                    if section.table.entries.contains_key(&object_number.number)
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

    /// Selects the nearest candidate that succeeds through the strict parser.
    fn closest_valid_candidate(
        &self,
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
            .find(|candidate| parse_section_at(&self.parser, *candidate).is_ok())
    }

    /// Checks whether `offset` starts a complete whitespace-terminated xref row.
    fn looks_like_malformed_entry(&self, offset: usize) -> bool {
        let Ok(mut recovery) = self.at_offset(offset) else {
            return false;
        };
        recovery.parser.skip_whitespace();
        recovery.parser.parse_cross_reference_entry().is_ok()
            && recovery
                .parser
                .remaining_input()
                .first()
                .copied()
                .is_some_and(PdfParser::is_pdf_whitespace)
    }

    /// Repairs normal entries according to the syntax of their source section.
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

    /// Rechecks and repairs every normal object offset against the source bytes.
    fn repair_offsets(&self, table: &mut CrossReferenceTable, policy: OffsetRepairPolicy) {
        let mut invalid_entries = Vec::new();

        for (&object_number, entry) in &mut table.entries {
            let CrossReferenceEntryType::Normal {
                byte_offset,
                generation_number,
            } = entry
            else {
                continue;
            };

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
                        .then(|| {
                            self.latest_indirect_object_offset(object_number, generation_number)
                        })
                        .flatten()
                });

            match recovered_offset {
                Some(offset) => {
                    *entry = CrossReferenceEntryType::new_normal(offset, generation_number)
                }
                None => invalid_entries.push(object_number),
            }
        }

        for object_number in invalid_entries {
            let _ = table.entries.remove(&object_number);
        }
    }

    /// Searches near a declared offset for the matching object header.
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

    /// Falls back to the newest matching object declaration in the complete file.
    fn latest_indirect_object_offset(
        &self,
        object_number: usize,
        generation_number: usize,
    ) -> Option<usize> {
        (0..self.parser.tokenizer.input.len())
            .rev()
            .find(|offset| self.is_indirect_object_at(*offset, object_number, generation_number))
    }

    /// Checks for the requested indirect object declaration at one byte offset.
    fn is_indirect_object_at(
        &self,
        offset: usize,
        object_number: usize,
        generation_number: usize,
    ) -> bool {
        self.parser
            .matches_indirect_object_header_at(offset, object_number, generation_number)
    }

    /// Rejects any merged normal entry that still disagrees with the source bytes.
    pub(crate) fn validate(
        &self,
        table: CrossReferenceTable,
        xref_offset: usize,
    ) -> Result<CrossReferenceTable, ParserError> {
        let is_valid = table.entries.iter().all(|(&object_number, entry)| {
            let CrossReferenceEntryType::Normal {
                byte_offset,
                generation_number,
            } = entry
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

    /// Creates an invalid-xref error at the recovery cursor's current position.
    fn invalid_xref_error(&self) -> ParserError {
        ParserError::InvalidXrefAtOffset {
            offset: self.parser.tokenizer.position,
        }
    }
}

#[cfg(test)]
mod tests {
    use pdf_object_reader::cross_reference_table::CrossReferenceEntryType;

    use super::*;

    /// Formats one textual xref row for recovery tests.
    fn format_xref_entry(offset: usize, generation: usize, status: char) -> Vec<u8> {
        format!("{offset:010} {generation:05} {status} \n").into_bytes()
    }

    #[test]
    fn malformed_entry_probe_restores_position_after_failure() {
        let parser = PdfParser::from(b"not-an-xref entry".as_slice())
            .at_offset(3)
            .unwrap();
        let mut recovery = XrefRecovery::new(&parser);
        recovery.parser.tokenizer.position = 3;

        let result = recovery.try_parse_malformed_entry();

        assert!(result.is_err());
        assert_eq!(recovery.parser.position(), 3);
    }

    #[test]
    fn malformed_entry_probe_parses_valid_entry() {
        let mut data = format_xref_entry(12, 34, 'n');
        data.extend_from_slice(b"trailer");
        let parser = PdfParser::from(data.as_slice());
        let mut recovery = XrefRecovery::new(&parser);

        let entry = recovery.try_parse_malformed_entry().unwrap();

        assert_eq!(entry, CrossReferenceEntryType::new_normal(12, 34));
        assert!(matches!(
            recovery.parser.remaining_input().first().copied(),
            Some(b' ') | Some(b'\n')
        ));
    }

    #[test]
    fn malformed_entry_detection_requires_terminator() {
        let data = format_xref_entry(12, 34, 'n');
        let parser = PdfParser::from(data.as_slice());
        let recovery = XrefRecovery::new(&parser);
        assert!(recovery.looks_like_malformed_entry(0));

        let truncated = b"0000000012 00034 n".to_vec();
        let parser = PdfParser::from(truncated.as_slice());
        let recovery = XrefRecovery::new(&parser);

        assert!(!recovery.looks_like_malformed_entry(0));
        assert_eq!(recovery.parser.position(), 0);
    }
}
