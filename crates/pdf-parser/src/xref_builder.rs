//! Strict cross-reference discovery and parsing.
//!
//! This module owns the normal PDF path: locating numeric `startxref` values,
//! parsing traditional tables or xref streams at exact offsets, and merging `/Prev`
//! and `/XRefStm` chains. Malformed-file heuristics live in the private
//! `xref_recovery` module and are invoked only when the strict operation they
//! supplement cannot succeed.
//!
//! PDF revisions are processed newest first. Entries from the first section that
//! mentions an object number remain authoritative; older `/Prev` sections fill only
//! missing numbers. In a hybrid-reference revision, the traditional table is merged
//! before its `/XRefStm` stream for the same reason. A set of visited offsets prevents
//! malformed cyclic chains from looping forever.
//!
//! Exact section parsing is deliberately separate from repair. `parse_section_at`
//! accepts only the syntax located at the supplied byte offset and never searches the
//! surrounding input. The builder delegates to the private `XrefRecovery` type when
//! exact parsing or offset validation fails, keeping the normal parser reusable by
//! recovery probes without embedding heuristics in it.

use std::collections::{BTreeMap, HashSet};

use pdf_object::{
    cross_reference_table::{CrossReferenceEntryType, CrossReferenceTable},
    object_resolver::PassthroughResolver,
    object_variant::ObjectVariant,
    stream::StreamObject,
    trailer::Trailer,
};

use crate::{error::ParserError, parser::PdfParser, xref_recovery::XrefRecovery};

const STARTXREF_KEYWORD: &[u8] = b"startxref";

/// Identifies the syntax used by a strictly parsed cross-reference section.
///
/// Recovery uses the source kind to choose an offset-repair policy: traditional tables
/// and xref streams have different failure modes and must not be repaired equally
/// aggressively.
#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) enum XrefSectionKind {
    /// A textual `xref` table followed by a trailer dictionary.
    Traditional,
    /// An indirect stream object whose dictionary has `/Type /XRef` semantics.
    Stream,
}

/// A strictly parsed cross-reference table paired with its original syntax.
///
/// The common table representation lets chain merging ignore whether entries came from
/// rows or decoded stream fields, while `kind` preserves the information recovery needs
/// when validating their offsets.
pub(crate) struct ParsedXrefSection {
    /// Syntax that produced `table`.
    pub(crate) kind: XrefSectionKind,
    /// Parsed entries and trailer metadata for this one revision section.
    pub(crate) table: CrossReferenceTable,
}

/// Coordinates cross-reference discovery, revision traversal, and precedence.
///
/// The builder owns an independent parser cursor over the complete input. Temporary
/// probes are forked from it, so building a table never mutates the user's parser.
struct XrefBuilder<'input> {
    parser: PdfParser<'input>,
}

impl<'input> XrefBuilder<'input> {
    /// Creates a builder around an already independent parser cursor.
    ///
    /// Callers normally provide a parser positioned at byte zero. The cursor itself is
    /// used only as the source for further independent probes.
    fn new(parser: PdfParser<'input>) -> Self {
        Self { parser }
    }

    /// Forks the builder at an absolute byte offset without changing this builder.
    ///
    /// This is used to validate individual `startxref` markers. Bounds failures are
    /// propagated as parser errors instead of creating a partial probe.
    fn at_offset(&self, offset: usize) -> Result<Self, ParserError> {
        Ok(Self::new(self.parser.at_offset(offset)?))
    }

    /// Builds the best complete cross-reference table reachable from the file tail.
    ///
    /// Every syntactically numeric `startxref` value is tried newest first. A candidate
    /// succeeds only when its revision chain can be merged and the resulting normal
    /// entries validate against real indirect-object headers. If a newer marker is
    /// corrupt, an older marker may still recover a usable revision.
    ///
    /// When no numeric marker exists at all, control passes to linear object
    /// reconstruction. A file that contains numeric markers but whose candidates all
    /// fail returns the last candidate error; it is not silently reinterpreted as a
    /// marker-less document.
    ///
    /// # Errors
    ///
    /// Returns the last section, chain, or validation error when all discovered offsets
    /// fail. Missing-marker reconstruction errors and input offset errors are propagated.
    fn build(&mut self) -> Result<CrossReferenceTable, ParserError> {
        let offsets = match self.startxref_offsets() {
            Ok(offsets) => offsets,
            Err(ParserError::MissingStartXref) => {
                return XrefRecovery::new(&self.parser).rebuild_without_xref();
            }
            Err(error) => return Err(error),
        };
        let mut last_error = None;

        for offset in offsets {
            match self
                .merge_chain(offset)
                .and_then(|table| XrefRecovery::new(&self.parser).validate(table, offset))
            {
                Ok(table) => return Ok(table),
                Err(error) => last_error = Some(error),
            }
        }

        Err(last_error.unwrap_or(ParserError::MissingStartXref))
    }

    /// Discovers syntactically valid numeric `startxref` declarations.
    ///
    /// Raw byte-window scanning first finds every possible keyword occurrence. Each
    /// occurrence is then parsed through [`PdfParser::read_keyword`], which rejects text
    /// embedded in a longer regular token, followed by an unsigned decimal offset.
    /// Malformed occurrences are ignored independently so junk near one marker cannot
    /// hide an older valid revision.
    ///
    /// Positions are traversed in reverse file order, making the returned offsets newest
    /// first. All parsing occurs on forked cursors and leaves `self.parser` unchanged.
    ///
    /// # Errors
    ///
    /// Returns [`ParserError::MissingStartXref`] if no occurrence contains a valid
    /// numeric value. It does not check whether any returned offset points to an xref
    /// section; that is the responsibility of [`Self::build`].
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
            let offset = self.at_offset(position).and_then(|mut builder| {
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

    /// Traverses and merges the revision chain beginning at `start_offset`.
    ///
    /// The current section is parsed before following its trailer's `/Prev`, so
    /// [`Self::merge_entries`] naturally preserves newer declarations. `/XRefStm`
    /// identifies an auxiliary xref stream in the same hybrid revision; its entries are
    /// merged after the traditional section, leaving traditional rows authoritative for
    /// duplicate object numbers.
    ///
    /// A single visited-offset set covers both links. This prevents cycles through
    /// `/Prev`, repeated auxiliary streams, and cross-links between the two. Trailer
    /// selection is handled separately because an otherwise useful newer trailer may
    /// omit `/Root` and rely on an earlier revision.
    ///
    /// # Errors
    ///
    /// Returns an error if a section cannot be parsed or recovered, `/Prev` or
    /// `/XRefStm` is not a usable direct number, or no trailer can be selected.
    fn merge_chain(&self, start_offset: usize) -> Result<CrossReferenceTable, ParserError> {
        let mut entries = BTreeMap::new();
        let mut visited_offsets = HashSet::new();
        let mut current_offset = start_offset;
        let mut trailer = None;
        let mut recovery = XrefRecovery::new(&self.parser);

        while visited_offsets.insert(current_offset) {
            let table = recovery.parse_section_with_recovery(current_offset)?;
            let auxiliary_offset = table.trailer.dictionary.get(b"XRefStm").cloned();
            let previous_offset = table.trailer.dictionary.get(b"Prev").cloned();

            // The loop walks newest to oldest. Inserting only vacant keys below makes
            // the first visible declaration of each object number authoritative.
            Self::merge_entries(&mut entries, table.entries);

            if let Some(offset) = auxiliary_offset {
                let offset = offset.try_number::<usize>(&PassthroughResolver)?;
                if visited_offsets.insert(offset) {
                    let auxiliary_table = recovery.parse_section_with_recovery(offset)?;
                    // Traditional entries are authoritative in hybrid revisions.
                    Self::merge_entries(&mut entries, auxiliary_table.entries);
                }
            }

            Self::prefer_newer_trailer(&mut trailer, table.trailer);

            let Some(previous_offset) = previous_offset else {
                break;
            };
            current_offset = previous_offset.try_number::<usize>(&PassthroughResolver)?;
        }

        let trailer = trailer.ok_or(ParserError::MissingStartXref)?;
        Ok(CrossReferenceTable::new(entries, trailer))
    }

    /// Adds entries that have not already been supplied by a newer section.
    ///
    /// Cross-reference precedence is keyed by object number, regardless of whether an
    /// entry is normal, free, or compressed. Consequently a newer free entry also blocks
    /// an older live declaration from being resurrected.
    fn merge_entries(
        entries: &mut BTreeMap<usize, CrossReferenceEntryType>,
        section_entries: BTreeMap<usize, CrossReferenceEntryType>,
    ) {
        for (object_number, entry) in section_entries {
            let _ = entries.entry(object_number).or_insert(entry);
        }
    }

    /// Selects the newest trailer capable of anchoring the document catalog.
    ///
    /// The first trailer encountered is the newest and is retained by default. It is
    /// replaced only when it lacks `/Root` and an older candidate contains one. Once a
    /// rooted trailer has been selected, still older metadata cannot replace it.
    ///
    /// This chooses one complete trailer rather than merging dictionary keys across
    /// revisions; individual inherited relationships are represented by the revision
    /// chain itself.
    fn prefer_newer_trailer(preferred: &mut Option<Trailer>, candidate: Trailer) {
        let should_replace = preferred
            .as_ref()
            .is_some_and(|trailer| trailer.dictionary.get(b"Root").is_none())
            && candidate.dictionary.get(b"Root").is_some();

        if preferred.is_none() || should_replace {
            *preferred = Some(candidate);
        }
    }
}

/// Strictly parses a cross-reference section at exactly `offset`.
///
/// Traditional tables begin directly with the `xref` object syntax. Xref streams begin
/// with an indirect-object header, so the initial identifier probe selects the correct
/// parsing route without consuming state on failure. Both routes use
/// [`PassthroughResolver`]: this stage parses section syntax but does not load arbitrary
/// referenced objects.
///
/// The resulting object must be either a traditional table or a stream decodable as an
/// xref stream. No surrounding search, offset adjustment, or entry repair occurs here.
/// This strict primitive is visible within the crate so recovery can test candidate
/// offsets without duplicating normal parsing behavior.
///
/// # Errors
///
/// Returns [`ParserError::InvalidXrefAtOffset`] when `offset` is outside the input or the
/// parsed object is not an xref section. Object, stream, filter, and xref-stream decoding
/// errors are otherwise propagated unchanged.
pub(crate) fn parse_section_at(
    parser: &PdfParser<'_>,
    offset: usize,
) -> Result<ParsedXrefSection, ParserError> {
    let mut parser = parser
        .at_offset(offset)
        .map_err(|_| ParserError::InvalidXrefAtOffset { offset })?;
    let object = match parser.parse_indirect_object_id() {
        Some(identifier) => parser.parse_indirect_object_value(identifier, &PassthroughResolver)?,
        None => parser.parse_object(&PassthroughResolver)?,
    };

    match object {
        ObjectVariant::CrossReferenceTable(table) => Ok(ParsedXrefSection {
            kind: XrefSectionKind::Traditional,
            table,
        }),
        ObjectVariant::Stream(stream) => parse_stream_section(stream),
        _ => Err(ParserError::InvalidXrefAtOffset { offset }),
    }
}

/// Decodes one parsed stream as a cross-reference stream section.
///
/// Stream filters and `/W`/`/Index` field interpretation are delegated to the dedicated
/// xref-stream parser. Tagging the result as [`XrefSectionKind::Stream`] preserves the
/// source-specific validation policy for recovery.
///
/// # Errors
///
/// Propagates stream decoding and malformed xref-stream dictionary or entry errors.
fn parse_stream_section(stream: StreamObject) -> Result<ParsedXrefSection, ParserError> {
    Ok(ParsedXrefSection {
        kind: XrefSectionKind::Stream,
        table: crate::cross_reference_stream::parse_xref_stream(stream, &PassthroughResolver)?,
    })
}

impl PdfParser<'_> {
    /// Locates cross-reference sections and merges their visible revisions.
    ///
    /// The parser searches `startxref` declarations newest first, follows `/Prev` and
    /// hybrid `/XRefStm` links, applies revision precedence, and validates the merged
    /// object offsets. Exact section parsing is always attempted before malformed
    /// sections or missing tables are delegated to isolated recovery.
    ///
    /// This method operates on independent cursors and leaves the parser position
    /// unchanged.
    ///
    /// # Errors
    ///
    /// Returns an error when neither the marker-driven path nor its applicable recovery
    /// strategy can produce a connected, internally valid cross-reference table.
    pub fn build_xref_table(&self) -> Result<CrossReferenceTable, ParserError> {
        XrefBuilder::new(self.at_offset(0)?).build()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies scanning for `startxref` does not mutate the caller's cursor.
    #[test]
    fn startxref_scan_restores_parser_position() {
        let data = b"startxref\n12\n";
        let parser = PdfParser::from(data.as_slice()).at_offset(4).unwrap();
        let mut builder = XrefBuilder::new(parser);

        let offsets = builder.startxref_offsets().unwrap();

        assert_eq!(offsets, vec![12]);
        assert_eq!(builder.parser.position(), 4);
    }
}
