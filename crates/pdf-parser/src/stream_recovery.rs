//! Recovery-only parsing for malformed PDF streams.
//!
//! The normal stream parser treats `/Length` as authoritative. That is the only
//! unambiguous way to parse a stream because arbitrary binary stream data may itself
//! contain the byte sequences `endstream`, `endobj`, or text resembling complete PDF
//! objects. See [`PdfParser::parse_stream`](crate::parser::PdfParser::parse_stream) for
//! that strict path.
//!
//! This module exists for document objects with a missing, indirect, stale, or otherwise
//! incorrect `/Length`. Refusing to scan past that metadata can discard otherwise usable
//! page resources, while trusting an incorrect value can leave a linear scanner in the
//! middle of the stream. The recovery methods below therefore use this strategy:
//!
//! 1. Prefer the declared byte boundary when it is available and structurally valid.
//! 2. Otherwise inspect delimiter-bounded `endstream` tokens.
//! 3. Accept only a token followed, apart from whitespace and comments, by `endobj`.
//!    Some writers truncate the keyword to `endstrea`; that form is accepted too.
//! 4. When `/Length` supplied a hint, choose the valid candidate nearest its declared
//!    boundary. Without a hint, choose the first valid candidate in byte order.
//!
//! Requiring `endobj` eliminates many accidental matches inside stream data, but it
//! cannot make scanning arbitrary bytes fully unambiguous. Consequently structural objects
//! such as cross-reference streams are parsed strictly first and fall back to this path
//! only when that fails, while ordinary document objects use it directly.

use pdf_object_reader::{
    dictionary::Dictionary, object_lookup::ObjectLookupExt, object_resolver::ObjectResolver,
    object_variant::ObjectVariant,
};

use crate::{error::ParserError, parser::PdfParser};

const STREAM_KEYWORD: &[u8] = b"stream";
const ENDSTREAM_KEYWORD: &[u8] = b"endstream";
/// The `endstream` keyword without its final byte, as emitted by some broken writers.
const TRUNCATED_ENDSTREAM_KEYWORD: &[u8] = b"endstrea";

impl PdfParser<'_> {
    /// Parses and returns a raw stream body using the recovery rules of this module.
    ///
    /// The parser must point at the `stream` keyword. The returned bytes are not filter
    /// decoded. On success the parser points immediately after the selected `endstream`
    /// keyword; the caller remains responsible for consuming `endobj`.
    ///
    /// Unlike [`PdfParser::parse_stream`](crate::parser::PdfParser::parse_stream), this
    /// method permits a missing or incorrect `/Length`. Indirect lengths can be resolved
    /// here because normal recovered-object loading has an object resolver available.
    /// Resolution errors are still returned rather than silently treated as an absent
    /// length.
    ///
    /// # Errors
    ///
    /// Returns an error if `stream` is missing, `/Length` cannot be interpreted or
    /// resolved, no structurally plausible `endstream` can be found, or the selected
    /// byte range is outside the input.
    pub(crate) fn parse_stream_recovering(
        &mut self,
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Vec<u8>, ParserError> {
        self.read_keyword(STREAM_KEYWORD)?;
        let stream_data_start = self.position();
        let declared_stream_end = dictionary
            .optional_number::<usize>(b"Length", objects)?
            .map(|length| stream_data_start.saturating_add(length));

        // A correct length remains authoritative. Besides preserving arbitrary binary
        // contents, this fast path avoids scanning the rest of a potentially large file.
        if let Some(stream_end) = declared_stream_end
            && try_exact_stream_end(self, stream_end)
        {
            return stream_bytes(self.tokenizer.input, stream_data_start, stream_end);
        }

        let (stream_data_end, terminator_end) =
            find_stream_end(self, stream_data_start, declared_stream_end)
                .ok_or(ParserError::UnexpectedEndOfFile)?;
        self.tokenizer.position = terminator_end;
        stream_bytes(self.tokenizer.input, stream_data_start, stream_data_end)
    }

    /// Advances past a stream while linearly reconstructing a cross-reference table.
    ///
    /// The linear scanner runs before indirect references can be resolved, so the stream
    /// boundary is located by [`Self::stream_range_with_direct_length`]. On success the
    /// parser points immediately after `endstream`; the scanner consumes the containing
    /// `endobj` separately.
    ///
    /// # Errors
    ///
    /// Returns an error if `stream` is missing or no structurally plausible terminator
    /// is present in the remaining input.
    pub(crate) fn skip_stream_recovering(
        &mut self,
        dictionary: &Dictionary,
    ) -> Result<(), ParserError> {
        self.stream_range_with_direct_length(dictionary).map(|_| ())
    }

    /// Parses and returns a raw stream body whose `/Length` cannot be resolved.
    ///
    /// Cross-reference streams are parsed before any object can be resolved, so, like
    /// [`Self::skip_stream_recovering`], only a direct `/Length` is used. On success the
    /// parser points immediately after `endstream`; the caller consumes `endobj`.
    ///
    /// # Errors
    ///
    /// Returns an error if `stream` is missing, no structurally plausible terminator is
    /// present in the remaining input, or the selected byte range is outside the input.
    pub(crate) fn parse_stream_recovering_direct_length(
        &mut self,
        dictionary: &Dictionary,
    ) -> Result<Vec<u8>, ParserError> {
        let (stream_data_start, stream_data_end) =
            self.stream_range_with_direct_length(dictionary)?;
        stream_bytes(self.tokenizer.input, stream_data_start, stream_data_end)
    }

    /// Locates a stream body using only a non-negative, directly stored `/Length`.
    ///
    /// Missing, invalid, and indirect lengths are treated as unavailable hints and trigger
    /// structural scanning. Returns the `(start, end)` byte range of the stream data and
    /// leaves the parser immediately after `endstream`.
    fn stream_range_with_direct_length(
        &mut self,
        dictionary: &Dictionary,
    ) -> Result<(usize, usize), ParserError> {
        self.read_keyword(STREAM_KEYWORD)?;
        let stream_data_start = self.position();
        let declared_stream_end =
            direct_stream_length(dictionary).map(|length| stream_data_start.saturating_add(length));

        if let Some(stream_end) = declared_stream_end
            && try_exact_stream_end(self, stream_end)
        {
            return Ok((stream_data_start, stream_end));
        }

        let (stream_data_end, terminator_end) =
            find_stream_end(self, stream_data_start, declared_stream_end)
                .ok_or(ParserError::UnexpectedEndOfFile)?;
        self.tokenizer.position = terminator_end;
        Ok((stream_data_start, stream_data_end))
    }
}

/// Extracts the only kind of `/Length` usable during the initial linear scan.
///
/// A negative integer cannot represent a byte count. References and other object types
/// are deliberately ignored: resolving them before the cross-reference table exists
/// would make reconstruction depend on the index it is trying to build.
fn direct_stream_length(dictionary: &Dictionary) -> Option<usize> {
    match dictionary.get(b"Length") {
        Some(ObjectVariant::Integer(length)) => usize::try_from(*length).ok(),
        _ => None,
    }
}

/// Validates the syntax following an alleged exact stream boundary.
///
/// A boundary is accepted only if an optional line ending is followed by `endstream`
/// and the enclosing `endobj`. The latter check is stricter than ordinary stream
/// parsing because recovery may otherwise accept an embedded keyword from binary data.
///
/// This function is transactional on failure: it restores the parser to its original
/// position. On success it intentionally leaves the parser immediately after
/// `endstream`, matching [`PdfParser::parse_stream_recovering`] and
/// [`PdfParser::skip_stream_recovering`].
fn try_exact_stream_end(parser: &mut PdfParser<'_>, stream_end: usize) -> bool {
    if stream_end > parser.tokenizer.input.len() {
        return false;
    }

    let mark = parser.position();
    parser.tokenizer.position = stream_end;
    parser.try_read_end_of_line_marker();
    let terminator_end = endstream_terminator_len(parser.tokenizer.input, parser.position())
        .map(|length| parser.position().saturating_add(length))
        .filter(|&terminator_end| endobj_follows(parser.tokenizer.input, terminator_end));
    match terminator_end {
        Some(terminator_end) => {
            parser.tokenizer.position = terminator_end;
            true
        }
        None => {
            parser.tokenizer.position = mark;
            false
        }
    }
}

/// Returns the length of the stream terminator keyword at `offset`, if one is present.
///
/// Both `endstream` and its truncated form `endstrea` are recognized, provided the
/// keyword ends at a PDF delimiter or the end of input.
fn endstream_terminator_len(input: &[u8], offset: usize) -> Option<usize> {
    let remaining = input.get(offset..)?;
    [ENDSTREAM_KEYWORD, TRUNCATED_ENDSTREAM_KEYWORD]
        .into_iter()
        .find(|keyword| {
            remaining.starts_with(keyword)
                && remaining
                    .get(keyword.len())
                    .copied()
                    .is_none_or(PdfParser::is_pdf_delimiter)
        })
        .map(<[u8]>::len)
}

/// Locates the best structurally plausible `endstream` terminator.
///
/// A candidate is `endstream` or its truncated form `endstrea`, and must:
///
/// - begin and end on PDF token boundaries; and
/// - be followed only by whitespace/comments before `endobj`.
///
/// If a declared boundary is available, every valid candidate is considered and the
/// one with the smallest absolute distance from that boundary wins. Ties prefer the
/// earlier candidate. If no boundary is available, byte-order iteration makes the
/// first valid candidate the best information recovery can provide.
///
/// The returned pair is `(stream_data_end, terminator_end)`. The first value excludes
/// the separator line ending immediately before the keyword; the second points just
/// past the keyword.
fn find_stream_end(
    parser: &PdfParser<'_>,
    stream_data_start: usize,
    declared_stream_end: Option<usize>,
) -> Option<(usize, usize)> {
    let input = parser.tokenizer.input;
    let mut best_candidate = None;

    for (relative_offset, window) in input
        .get(stream_data_start..)?
        .windows(TRUNCATED_ENDSTREAM_KEYWORD.len())
        .enumerate()
    {
        if window != TRUNCATED_ENDSTREAM_KEYWORD {
            continue;
        }

        let endstream_offset = stream_data_start.saturating_add(relative_offset);
        // `windows` also finds the letters inside names and binary data. PDF delimiter
        // checks ensure that only a standalone keyword proceeds to structural testing.
        // The search range itself may begin with an empty stream's terminator, so that
        // local start remains a valid boundary even when it is not byte zero.
        let has_leading_boundary =
            endstream_offset == stream_data_start || parser.is_token_start_at(endstream_offset);
        let Some(terminator_end) = endstream_terminator_len(input, endstream_offset)
            .map(|length| endstream_offset.saturating_add(length))
        else {
            continue;
        };
        if !has_leading_boundary || !endobj_follows(input, terminator_end) {
            continue;
        }

        let stream_data_end = trim_stream_data_end(input, stream_data_start, endstream_offset);
        let Some(declared_stream_end) = declared_stream_end else {
            return Some((stream_data_end, terminator_end));
        };

        // A stale length is still useful as a proximity hint. This reduces the chance
        // of selecting object-like text embedded much earlier or later in the payload.
        let distance = endstream_offset.abs_diff(declared_stream_end);
        match best_candidate {
            Some((best_offset, _, _, best_distance))
                if best_distance < distance
                    || (best_distance == distance && best_offset <= endstream_offset) => {}
            _ => {
                best_candidate =
                    Some((endstream_offset, stream_data_end, terminator_end, distance));
            }
        }
    }

    best_candidate.map(|(_, data_end, terminator_end, _)| (data_end, terminator_end))
}

/// Checks whether an `endstream` candidate is followed by its containing `endobj`.
///
/// A separate parser keeps this look-ahead from mutating the recovery parser. Standard
/// PDF whitespace and comments between the two keywords are permitted.
fn endobj_follows(input: &[u8], position: usize) -> bool {
    let Some(remaining) = input.get(position..) else {
        return false;
    };
    let mut probe = PdfParser::from(remaining);
    probe.skip_whitespace_and_comments();
    probe.read_keyword(b"endobj").is_ok()
}

/// Removes the syntax separator immediately before a recovered `endstream` keyword.
///
/// PDF writers conventionally place `endstream` on a new line. When the true length is
/// unavailable, that final LF, CR, or CRLF cannot be distinguished through byte counts;
/// it is treated as the separator and excluded from the recovered payload. At most one
/// line-ending sequence is removed, and never any byte before `stream_data_start`.
fn trim_stream_data_end(input: &[u8], stream_data_start: usize, endstream_offset: usize) -> usize {
    if endstream_offset <= stream_data_start {
        return endstream_offset;
    }

    let last_byte_offset = endstream_offset.saturating_sub(1);
    match input.get(last_byte_offset).copied() {
        Some(b'\n') => match last_byte_offset
            .checked_sub(1)
            .and_then(|index| input.get(index))
        {
            Some(b'\r') if last_byte_offset.saturating_sub(1) >= stream_data_start => {
                last_byte_offset.saturating_sub(1)
            }
            _ => last_byte_offset,
        },
        Some(b'\r') => last_byte_offset,
        _ => endstream_offset,
    }
}

/// Copies a previously validated stream range from the parser's immutable input.
///
/// Keeping the bounds check here ensures malformed offsets become parser errors rather
/// than indexing panics. A new vector is required because parsed stream objects own
/// their raw bytes independently of the input parser.
fn stream_bytes(
    input: &[u8],
    stream_data_start: usize,
    stream_data_end: usize,
) -> Result<Vec<u8>, ParserError> {
    input
        .get(stream_data_start..stream_data_end)
        .map(<[u8]>::to_vec)
        .ok_or(ParserError::UnexpectedEndOfFile)
}
