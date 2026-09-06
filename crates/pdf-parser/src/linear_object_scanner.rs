//! Linear object discovery for PDFs without a usable cross-reference table.
//!
//! A PDF normally locates indirect objects through `startxref` and one or more
//! cross-reference sections. When that index is absent, the parser has no trustworthy
//! offsets and must reconstruct a minimal table from the file bytes. This module owns
//! that recovery-only scan.
//!
//! The scanner does not accept every object-shaped byte sequence it encounters. At a
//! PDF token boundary it probes a complete indirect object or trailer with an
//! independent parser. A successful object probe advances past the entire object,
//! including any stream body, so syntax embedded inside a stream is not indexed as a
//! top-level object. A failed probe consumes only one byte in the outer scanner and the
//! search continues without committing partial parser state.
//!
//! Duplicate object numbers are resolved in file order: later complete declarations
//! replace earlier ones, matching the usual incremental-update semantics. Likewise,
//! the last successfully parsed trailer that directly contains `/Root` is retained.
//! Recovery succeeds only when that root reference names an object discovered by the
//! scan. These checks reduce false positives, but arbitrary byte scanning is inherently
//! less authoritative than a valid cross-reference section; callers should enter this
//! path only after normal xref discovery has failed.

use std::collections::BTreeMap;

use pdf_object_reader::{
    cross_reference_table::{CrossReferenceEntryType, CrossReferenceTable},
    object_id::ObjectId,
    object_resolver::PassthroughResolver,
    object_variant::ObjectVariant,
    trailer::Trailer,
};

use crate::{error::ParserError, parser::PdfParser};

const TRAILER_KEYWORD: &[u8] = b"trailer";
const STREAM_KEYWORD: &[u8] = b"stream";

/// A complete top-level construct accepted by a recovery probe.
///
/// The scanner retains only metadata needed to reconstruct a cross-reference table.
/// Parsed object values are intentionally discarded to avoid building a second object
/// store during indexing.
enum ScanCandidate {
    /// An indirect object declaration and the generation recorded in its header.
    IndirectObject {
        object_number: usize,
        generation_number: usize,
    },
    /// A trailer dictionary that directly exposes the document catalog through `/Root`.
    Trailer(Trailer),
}

/// Reconstructs cross-reference entries by scanning complete top-level PDF constructs.
///
/// The scanner owns an independent [`PdfParser`] over the original immutable input.
/// It never changes the cursor or nesting state of the parser supplied to [`Self::new`].
pub(crate) struct LinearObjectScanner<'input> {
    parser: PdfParser<'input>,
}

impl<'input> LinearObjectScanner<'input> {
    /// Creates an independent recovery scanner over the parser's complete input.
    ///
    /// Scanning always begins at byte zero, regardless of the supplied parser's current
    /// position. Only the input slice and its lifetime are shared.
    pub(crate) fn new(parser: &PdfParser<'input>) -> Self {
        Self {
            parser: PdfParser::from(parser.tokenizer.input),
        }
    }

    /// Scans the input and constructs a minimal usable cross-reference table.
    ///
    /// Whitespace, comments, file headers, and unrelated bytes are skipped. At each
    /// plausible token start, [`Self::candidate_at`] attempts a non-mutating complete
    /// parse. Successful objects advance the outer cursor past their full extent;
    /// unsuccessful probes advance it by one byte so later valid declarations remain
    /// discoverable.
    ///
    /// Inserting entries in byte order means the newest complete declaration of an
    /// object number wins. The same rule retains the newest rooted trailer. A table is
    /// returned only if at least one object was found and the retained trailer's `/Root`
    /// points to one of those objects.
    ///
    /// # Errors
    ///
    /// Returns [`ParserError::MissingStartXref`] when no rooted, internally connected
    /// reconstruction can be produced. Parser offset errors are propagated if an
    /// accepted candidate reports an invalid end position.
    pub(crate) fn scan(&self) -> Result<CrossReferenceTable, ParserError> {
        let mut objects = BTreeMap::new();
        let mut trailer = None;
        let mut scanner = self.parser.at_offset(0)?;

        while scanner.peek_byte().is_some() {
            // Comments must be skipped as units; otherwise object-shaped comment text
            // could be revisited one byte at a time and accepted as top-level syntax.
            scanner.skip_whitespace_and_comments();
            if scanner.peek_byte().is_none() {
                break;
            }
            if scanner.skip_eof_marker_as_comment() {
                // Incremental PDFs may contain older %%EOF markers before newer
                // revisions. Treat them as separators and continue scanning.
                continue;
            }

            let position = scanner.position();
            let Some((candidate, candidate_end)) = self.candidate_at(position) else {
                // A failed probe is deliberately non-committing. Moving one byte keeps
                // the scan exhaustive without trusting a partial parse.
                let _ = scanner.read_byte();
                continue;
            };

            match candidate {
                ScanCandidate::IndirectObject {
                    object_number,
                    generation_number,
                } => {
                    // BTreeMap::insert replaces an older declaration of the same object
                    // number, which models incremental updates encountered in file order.
                    let _ = objects.insert(
                        object_number,
                        CrossReferenceEntryType::new_normal(position, generation_number),
                    );
                }
                // Later rooted trailers supersede earlier revisions.
                ScanCandidate::Trailer(candidate) => trailer = Some(candidate),
            }
            // Resume after the complete accepted construct. In particular, this skips
            // any object-like bytes that appeared inside its stream payload.
            scanner = self.parser.at_offset(candidate_end)?;
        }

        let trailer = trailer.ok_or(ParserError::MissingStartXref)?;
        if objects.is_empty() || !root_is_indexed(&objects, &trailer) {
            return Err(ParserError::MissingStartXref);
        }

        Ok(CrossReferenceTable::new(objects, trailer))
    }

    /// Classifies and completely probes a candidate beginning at `position`.
    ///
    /// Only ASCII digits and `t` can begin constructs relevant to reconstruction, so
    /// the first-byte dispatch avoids invoking the object parser at every input byte.
    /// The token-boundary check rejects matches embedded in longer regular tokens.
    ///
    /// Returns the accepted construct together with the absolute position immediately
    /// after it. Any syntax or bounds error makes the position a non-candidate.
    fn candidate_at(&self, position: usize) -> Option<(ScanCandidate, usize)> {
        if !self.parser.is_token_start_at(position) {
            return None;
        }

        match self.parser.tokenizer.input.get(position).copied()? {
            byte if byte.is_ascii_digit() => self.indirect_object_at(position),
            b't' => self.rooted_trailer_at(position),
            _ => None,
        }
    }

    /// Probes a complete indirect object beginning at `position`.
    ///
    /// A fresh parser makes the probe transactional: failure cannot disturb the outer
    /// scan. On success only the identifier and final cursor are retained; the parsed
    /// value existed solely to prove the declaration's extent and structural validity.
    fn indirect_object_at(&self, position: usize) -> Option<(ScanCandidate, usize)> {
        let mut probe = self.parser.at_offset(position).ok()?;
        let identifier = self.scan_indirect_object(&mut probe).ok()?;
        Some((
            ScanCandidate::IndirectObject {
                object_number: identifier.number,
                generation_number: identifier.generation,
            },
            probe.position(),
        ))
    }

    /// Parses one indirect object far enough to determine its complete byte extent.
    ///
    /// The cross-reference table does not exist yet, so ordinary references are parsed
    /// through [`PassthroughResolver`] and are not followed. For stream objects, the
    /// value must be a dictionary and [`PdfParser::skip_stream_recovering`] locates the
    /// containing `endstream` without resolving an indirect `/Length`. Stream objects
    /// require an explicit `endobj`, preventing recovered stream terminators from
    /// swallowing or manufacturing adjacent objects. Non-stream objects retain the
    /// parser's narrowly defined implicit-`endobj` recovery behavior.
    ///
    /// On success `probe` points immediately after the object terminator.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid indirect-object header, malformed value, stream
    /// without a dictionary, unrecoverable stream boundary, or invalid terminator.
    fn scan_indirect_object(&self, probe: &mut PdfParser<'input>) -> Result<ObjectId, ParserError> {
        let object_start = probe.position();
        let identifier = probe.parse_indirect_object_id().ok_or(
            ParserError::ExpectedIndirectObjectDeclaration {
                position: object_start,
            },
        )?;
        let object = probe.parse_object(&PassthroughResolver)?;
        probe.skip_whitespace_and_comments();

        if probe.remaining_input().starts_with(STREAM_KEYWORD) {
            let ObjectVariant::Dictionary(dictionary) = object else {
                return Err(ParserError::StreamObjectWithoutDictionary);
            };
            probe.skip_stream_recovering(&dictionary)?;
            probe.consume_required_endobj()?;
        } else {
            probe.consume_endobj_or_implicit_boundary()?;
        }

        Ok(identifier)
    }

    /// Probes a complete trailer that directly exposes `/Root`.
    ///
    /// Checking the literal prefix is a cheap rejection before constructing a parser.
    /// [`PdfParser::parse_trailer`] then validates the complete trailer syntax. Trailers
    /// that rely on an older trailer for `/Root` are ignored because a linear recovery
    /// table needs one self-contained anchor to the catalog.
    ///
    /// Returns the parsed trailer and the absolute end of the probe, or `None` for any
    /// mismatch, parsing error, or missing direct `/Root` entry.
    fn rooted_trailer_at(&self, position: usize) -> Option<(ScanCandidate, usize)> {
        let input = self.parser.tokenizer.input.get(position..)?;
        if !input.starts_with(TRAILER_KEYWORD) {
            return None;
        }

        let mut probe = self.parser.at_offset(position).ok()?;
        let trailer = probe.parse_trailer(&PassthroughResolver).ok()?;
        let _ = trailer.dictionary.get(b"Root")?;
        Some((ScanCandidate::Trailer(trailer), probe.position()))
    }
}

/// Checks that the recovered trailer's catalog reference is present in the new index.
///
/// Merely finding a `/Root` key is insufficient: arbitrary stream bytes can resemble a
/// trailer. Requiring its referenced object number to have a complete scanned
/// declaration gives the reconstructed table a minimally connected document root.
/// Generation matching is intentionally left to normal object loading, consistent with
/// the cross-reference table's object-number keying.
fn root_is_indexed(objects: &BTreeMap<usize, CrossReferenceEntryType>, trailer: &Trailer) -> bool {
    matches!(
        trailer.dictionary.get(b"Root"),
        Some(ObjectVariant::Reference(object_number)) if objects.contains_key(&object_number.number)
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn scans_objects_and_prefers_the_latest_declaration() {
        let input =
            b"%PDF-1.7\n1 0 obj\nnull\nendobj\n1 2 obj\n42\nendobj\ntrailer\n<< /Root 1 0 R >>\n";
        let latest_offset = input
            .windows(b"1 2 obj".len())
            .position(|window| window == b"1 2 obj")
            .unwrap();

        let table = LinearObjectScanner::new(&PdfParser::from(input.as_slice()))
            .scan()
            .unwrap();
        let object = table.entries.get(&1).expect("object 1 should be indexed");

        assert_eq!(
            object,
            &CrossReferenceEntryType::new_normal(latest_offset, 2)
        );
    }

    #[test]
    fn skips_object_syntax_inside_a_direct_length_stream() {
        let fake_object = b"99 0 obj\nnull\nendobj\n";
        let input = format!(
            "%PDF-1.7\n1 0 obj\n<< /Length {} >>\nstream\n{}endstream\nendobj\ntrailer\n<< /Root 1 0 R >>\n",
            fake_object.len(),
            String::from_utf8_lossy(fake_object)
        );

        let table = LinearObjectScanner::new(&PdfParser::from(input.as_bytes()))
            .scan()
            .unwrap();

        assert_eq!(table.entries.len(), 1);
        assert!(!table.entries.contains_key(&99));
    }

    #[test]
    fn skips_object_syntax_inside_top_level_comments() {
        let input = b"%PDF-1.7\n% 99 0 obj null endobj\n1 0 obj\nnull\nendobj\ntrailer\n<< /Root 1 0 R >>\n";

        let table = LinearObjectScanner::new(&PdfParser::from(input.as_slice()))
            .scan()
            .unwrap();

        assert_eq!(table.entries.len(), 1);
        assert!(!table.entries.contains_key(&99));
    }

    #[test]
    fn skips_stream_with_a_forward_indirect_length() {
        let payload = b"endstream\n99 0 obj\nnull\nendobj\n";
        let input = format!(
            "%PDF-1.7\n1 0 obj\n<< /Length 2 0 R >>\nstream\n{}endstream\nendobj\n2 0 obj\n{}\nendobj\ntrailer\n<< /Root 1 0 R >>\n",
            String::from_utf8_lossy(payload),
            payload.len()
        );

        let table = LinearObjectScanner::new(&PdfParser::from(input.as_bytes()))
            .scan()
            .unwrap();

        assert_eq!(table.entries.len(), 2);
        assert!(table.entries.contains_key(&1));
        assert!(table.entries.contains_key(&2));
        assert!(!table.entries.contains_key(&99));
    }

    #[test]
    fn recovers_stream_with_missing_length() {
        let payload = b"endstream\n99 0 obj\nnull\nendobj\n";
        let input = format!(
            "%PDF-1.7\n1 0 obj\n<< >>\nstream\n{}endstream\nendobj\ntrailer\n<< /Root 1 0 R >>\n",
            String::from_utf8_lossy(payload),
        );

        let table = LinearObjectScanner::new(&PdfParser::from(input.as_bytes()))
            .scan()
            .unwrap();

        assert_eq!(table.entries.len(), 1);
        assert!(table.entries.contains_key(&1));
        assert!(!table.entries.contains_key(&99));
    }

    #[test]
    fn recovers_stream_with_incorrect_direct_length() {
        let input = b"%PDF-1.7\n1 0 obj\n<< /Length 2 >>\nstream\nHello World\nendstream\nendobj\ntrailer\n<< /Root 1 0 R >>\n";

        let table = LinearObjectScanner::new(&PdfParser::from(input.as_slice()))
            .scan()
            .unwrap();

        assert_eq!(table.entries.len(), 1);
        assert!(table.entries.contains_key(&1));
    }

    #[test]
    fn requires_a_trailer_root_that_was_indexed() {
        let input = b"%PDF-1.7\n1 0 obj\nnull\nendobj\ntrailer\n<< /Root 2 0 R >>\n";

        let error = LinearObjectScanner::new(&PdfParser::from(input.as_slice()))
            .scan()
            .expect_err("the missing root object should reject recovery");

        assert_eq!(error, ParserError::MissingStartXref);
    }
}
