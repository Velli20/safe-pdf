//! Recovery-only parsing for malformed PDF streams.
//!
//! The strict [`PdfParser::parse_stream`](crate::parser::PdfParser::parse_stream) treats
//! `/Length` as authoritative. The recovery path here also accepts a missing, indirect,
//! stale, or otherwise incorrect `/Length`:
//!
//! 1. A declared boundary followed by `endstream` and `endobj` is used as is.
//! 2. Otherwise the body ends at a standalone `endstream` keyword followed, apart from
//!    whitespace and comments, by `endobj`. Some writers truncate the keyword to
//!    `endstrea`; that form is accepted too.
//! 3. With a `/Length` hint, the terminator nearest the declared boundary wins; without
//!    one, the first in byte order.
//!
//! Requiring `endobj` rejects most accidental matches inside binary stream data, but
//! cannot rule them out. Structural objects such as cross-reference streams are therefore
//! parsed strictly first and fall back to this path only when that fails.

use std::ops::Range;

use pdf_object_reader::{
    dictionary::Dictionary, object_lookup::ObjectLookupExt, object_resolver::ObjectResolver,
    object_variant::ObjectVariant,
};

use crate::{error::ParserError, parser::PdfParser};

const STREAM_KEYWORD: &[u8] = b"stream";
/// The `endstream` keyword without its final byte, the shortest terminator accepted.
const TRUNCATED_ENDSTREAM_KEYWORD: &[u8] = b"endstrea";

impl PdfParser<'_> {
    /// Parses a raw stream body, resolving an indirect `/Length` through `objects`.
    ///
    /// Boundary rules and the final parser position are those of
    /// [`Self::recover_stream_range`]. The returned bytes are not filter decoded.
    ///
    /// # Errors
    ///
    /// Returns an error if `/Length` cannot be interpreted or resolved, or if
    /// [`Self::recover_stream_range`] fails.
    pub(crate) fn parse_stream_recovering(
        &mut self,
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Vec<u8>, ParserError> {
        let length = dictionary.optional_number::<usize>(b"Length", objects)?;
        self.recovered_stream_bytes(length)
    }

    /// Parses a raw stream body using only a direct `/Length`.
    ///
    /// Used for objects parsed before any reference can be resolved; see
    /// [`direct_stream_length`].
    ///
    /// # Errors
    ///
    /// Returns an error if [`Self::recover_stream_range`] fails.
    pub(crate) fn parse_stream_recovering_direct_length(
        &mut self,
        dictionary: &Dictionary,
    ) -> Result<Vec<u8>, ParserError> {
        self.recovered_stream_bytes(direct_stream_length(dictionary))
    }

    /// Locates a stream body using `length` only as a hint.
    ///
    /// The parser must point at the `stream` keyword. Returns the byte range of the
    /// stream data and leaves the parser immediately after the terminating `endstream`;
    /// the caller remains responsible for consuming `endobj`.
    ///
    /// # Errors
    ///
    /// Returns an error if `stream` is missing or no structurally plausible terminator
    /// is present in the remaining input.
    pub(crate) fn recover_stream_range(
        &mut self,
        length: Option<usize>,
    ) -> Result<Range<usize>, ParserError> {
        self.read_keyword(STREAM_KEYWORD)?;
        let input = self.tokenizer.input;
        let data_start = self.position();
        let declared_end = length.map(|length| data_start.saturating_add(length));

        // A correct length remains authoritative. Besides preserving arbitrary binary
        // contents, this avoids scanning the rest of a potentially large file.
        if let Some(data_end) = declared_end
            && let Some(terminator) = StreamTerminator::after_data(input, data_end)
        {
            self.tokenizer.position = terminator.end;
            return Ok(data_start..data_end);
        }

        let terminator = StreamTerminator::nearest(input, data_start, declared_end)
            .ok_or(ParserError::UnexpectedEndOfFile)?;
        self.tokenizer.position = terminator.end;
        Ok(data_start..trim_line_ending(input, data_start, terminator.start))
    }

    /// Recovers a stream body and copies it out of the parser's input.
    fn recovered_stream_bytes(&mut self, length: Option<usize>) -> Result<Vec<u8>, ParserError> {
        let range = self.recover_stream_range(length)?;
        self.tokenizer
            .input
            .get(range)
            .map(<[u8]>::to_vec)
            .ok_or(ParserError::UnexpectedEndOfFile)
    }
}

/// Extracts the only kind of `/Length` usable before references can be resolved.
///
/// A negative integer cannot represent a byte count. References are deliberately
/// ignored: resolving them while reconstructing the cross-reference table would make
/// reconstruction depend on the index it is trying to build.
pub(crate) fn direct_stream_length(dictionary: &Dictionary) -> Option<usize> {
    match dictionary.get(b"Length") {
        Some(ObjectVariant::Integer(length)) => usize::try_from(*length).ok(),
        _ => None,
    }
}

/// The byte span of an `endstream` keyword that is followed by `endobj`.
#[derive(Clone, Copy)]
struct StreamTerminator {
    start: usize,
    end: usize,
}

impl StreamTerminator {
    /// Reads a terminator beginning exactly at `start`.
    ///
    /// The keyword is `endstream` or `endstrea`, must end at a PDF delimiter or the end
    /// of input, and may be followed only by whitespace and comments before `endobj`.
    fn at(input: &[u8], start: usize) -> Option<Self> {
        let rest = input
            .get(start..)?
            .strip_prefix(TRUNCATED_ENDSTREAM_KEYWORD)?;
        let rest = rest.strip_prefix(b"m").unwrap_or(rest);
        let ends_keyword = rest
            .first()
            .copied()
            .is_none_or(PdfParser::is_pdf_delimiter);
        (ends_keyword && endobj_follows(rest)).then(|| Self {
            start,
            end: input.len().saturating_sub(rest.len()),
        })
    }

    /// Reads a terminator after stream data ending at `data_end`, allowing one line ending.
    fn after_data(input: &[u8], data_end: usize) -> Option<Self> {
        let rest = input.get(data_end..)?;
        let line_ending_len = match rest {
            [b'\r', b'\n', ..] => 2,
            [b'\r' | b'\n', ..] => 1,
            _ => 0,
        };
        Self::at(input, data_end.saturating_add(line_ending_len))
    }

    /// Returns every terminator beginning on a token boundary at or after `data_start`,
    /// in byte order.
    fn scan(input: &[u8], data_start: usize) -> impl Iterator<Item = Self> + '_ {
        input
            .get(data_start..)
            .unwrap_or_default()
            .windows(TRUNCATED_ENDSTREAM_KEYWORD.len())
            .enumerate()
            .filter(|(_, window)| *window == TRUNCATED_ENDSTREAM_KEYWORD)
            .map(move |(offset, _)| data_start.saturating_add(offset))
            .filter(|&start| {
                start
                    .checked_sub(1)
                    .and_then(|index| input.get(index))
                    .copied()
                    .is_some_and(PdfParser::is_pdf_delimiter)
            })
            .filter_map(|start| Self::at(input, start))
    }

    /// Selects the terminator nearest `declared_end`, or the first one without a hint.
    ///
    /// Ties prefer the earlier terminator. Candidates arrive in byte order, so the scan
    /// stops at the first one at or past the declared boundary: every later candidate
    /// lies farther away.
    fn nearest(input: &[u8], data_start: usize, declared_end: Option<usize>) -> Option<Self> {
        let mut candidates = Self::scan(input, data_start);
        let Some(declared_end) = declared_end else {
            return candidates.next();
        };

        let mut before = None;
        for candidate in candidates {
            if candidate.start < declared_end {
                before = Some(candidate);
                continue;
            }
            return Some(match before {
                Some(earlier)
                    if declared_end.abs_diff(earlier.start)
                        <= candidate.start.abs_diff(declared_end) =>
                {
                    earlier
                }
                _ => candidate,
            });
        }
        before
    }
}

/// Checks whether `input` begins, apart from whitespace and comments, with `endobj`.
fn endobj_follows(input: &[u8]) -> bool {
    let mut probe = PdfParser::from(input);
    probe.skip_whitespace_and_comments();
    probe.read_keyword(b"endobj").is_ok()
}

/// Returns the stream data end before a recovered terminator at `terminator_start`.
///
/// Without a trustworthy length, the final LF, CR, or CRLF before `endstream` is treated
/// as the separator and excluded from the payload. At most one line ending is removed,
/// and never any byte before `data_start`.
fn trim_line_ending(input: &[u8], data_start: usize, terminator_start: usize) -> usize {
    let Some(data) = input.get(data_start..terminator_start) else {
        return terminator_start;
    };
    let trimmed = data
        .strip_suffix(b"\r\n")
        .or_else(|| data.strip_suffix(b"\n"))
        .or_else(|| data.strip_suffix(b"\r"))
        .unwrap_or(data);
    data_start.saturating_add(trimmed.len())
}
