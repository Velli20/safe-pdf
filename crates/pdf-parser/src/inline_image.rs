use pdf_object_reader::dictionary::Dictionary;

use crate::{error::ParserError, parser::PdfParser};

const INLINE_IMAGE_DATA_BEGIN: &[u8] = b"ID";
const INLINE_IMAGE_DATA_END: &[u8] = b"EI";

/// What an inline image's dictionary says about where its data ends, used to find the
/// `EI` that closes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InlineImageDataEnd {
    /// The byte count of an unfiltered image's samples.
    Length(usize),
    /// The end-of-data marker of the first filter (`~>` for ASCII85, `>` for ASCIIHex).
    ///
    /// Such data holds whitespace and delimiters, so an `EI` keyword can appear inside
    /// it; only an `EI` after the marker ends the data.
    Marker(&'static [u8]),
    /// The dictionary does not determine where the data ends.
    Unknown,
}

impl PdfParser<'_> {
    /// Parses an inline image dictionary after the `BI` operator, through `ID` and the
    /// separator that follows it.
    ///
    /// Keys and values are kept as written; expanding abbreviations is left to the
    /// image decoder.
    pub fn parse_inline_image_dictionary(&mut self) -> Result<Dictionary, ParserError> {
        let dictionary = self.parse_inline_image_entries()?;
        self.read_keyword_with_optional_eol(INLINE_IMAGE_DATA_BEGIN, false)?;
        self.consume_inline_image_data_separator()?;
        Ok(dictionary)
    }

    /// Reads raw inline-image bytes through the `EI` terminator, which is consumed.
    ///
    /// `end` is what the image's dictionary says about where the data ends. A length is
    /// trusted only when `EI` follows it, and a marker only when the data contains it;
    /// otherwise the terminator is searched for from the start of the data.
    pub fn parse_inline_image_data(
        &mut self,
        end: InlineImageDataEnd,
    ) -> Result<Vec<u8>, ParserError> {
        let data_start = self.tokenizer.position;
        let data_end = self.find_inline_image_data_end(data_start, end)?;
        Ok(self.inline_image_data(data_start, data_end))
    }

    /// Consumes the mandatory separator immediately after the `ID` keyword.
    ///
    /// PDF inline-image syntax requires whitespace after `ID` before data begins.
    /// Enforcing this avoids ambiguous parses where data bytes could be mistaken for
    /// dictionary or operator content.
    fn consume_inline_image_data_separator(&mut self) -> Result<(), ParserError> {
        let Some(first) = self.tokenizer.peek_byte() else {
            return Err(ParserError::InlineImageMissingDataEnd);
        };

        if !Self::is_pdf_whitespace(first) {
            return Err(ParserError::InlineImageMissingDataSeparator {
                found: first,
                position: self.tokenizer.position,
            });
        }

        if matches!(first, b'\r' | b'\n') {
            self.try_read_end_of_line_marker();
        } else {
            let _ = self.tokenizer.read_exactly(1)?;
        }

        Ok(())
    }

    /// Finds the first valid inline-image `EI` end marker from the current position.
    ///
    /// We scan `E` candidates using byte iteration for efficiency, then validate each
    /// candidate with two guards:
    /// 1) preceding byte must be PDF whitespace (prevents common binary false matches),
    /// 2) `read_keyword("EI")` must succeed (enforces trailing delimiter semantics).
    ///
    /// Returning the absolute candidate offset allows callers to slice image data
    /// without recomputing ranges.
    fn find_inline_image_data_end(
        &mut self,
        data_start: usize,
        end: InlineImageDataEnd,
    ) -> Result<usize, ParserError> {
        let scan_start = match end {
            InlineImageDataEnd::Length(length) => {
                if let Some(data_end) =
                    self.try_find_inline_image_data_end_by_length(data_start, length)?
                {
                    return Ok(data_end);
                }
                data_start
            }
            InlineImageDataEnd::Marker(marker) => self
                .end_of_data_marker_end(data_start, marker)
                .unwrap_or(data_start),
            InlineImageDataEnd::Unknown => data_start,
        };

        self.find_inline_image_data_end_by_scan(scan_start)
    }

    /// Returns the offset just past the first `marker` at or after `data_start`.
    fn end_of_data_marker_end(&self, data_start: usize, marker: &[u8]) -> Option<usize> {
        let offset = self
            .tokenizer
            .input
            .get(data_start..)?
            .windows(marker.len())
            .position(|window| window == marker)?;
        data_start.checked_add(offset)?.checked_add(marker.len())
    }

    /// Resolves the inline-image end using the exact payload length.
    fn try_find_inline_image_data_end_by_length(
        &mut self,
        data_start: usize,
        expected_length: usize,
    ) -> Result<Option<usize>, ParserError> {
        let Some(candidate_start) = data_start.checked_add(expected_length) else {
            return Err(ParserError::InlineImageMissingDataEnd);
        };

        if self.try_consume_inline_image_end(candidate_start) {
            return Ok(Some(candidate_start));
        }

        Ok(None)
    }

    /// Finds the first valid inline-image `EI` end marker at or after `start` by scanning
    /// binary data.
    ///
    /// The scan first keeps the conservative whitespace-before-`EI` guard so binary or
    /// compressed inline-image payloads do not terminate early on embedded `EI` byte
    /// sequences. Some producers write `EI` directly after the binary data, so when no
    /// guarded end exists the scan accepts the first `EI` keyword without that guard.
    fn find_inline_image_data_end_by_scan(&mut self, start: usize) -> Result<usize, ParserError> {
        let input = self.tokenizer.input;
        let candidates = || {
            input
                .get(start..)
                .unwrap_or(&[])
                .iter()
                .enumerate()
                .filter_map(move |(offset, byte)| {
                    (*byte == b'E').then_some(start.saturating_add(offset))
                })
        };

        self.find_inline_image_end_candidate(candidates(), true)
            .or_else(|| self.find_inline_image_end_candidate(candidates(), false))
            .ok_or(ParserError::InlineImageMissingDataEnd)
    }

    /// Returns the first candidate offset that holds a valid `EI` keyword, optionally
    /// requiring PDF whitespace immediately before it.
    fn find_inline_image_end_candidate(
        &mut self,
        mut candidates: impl Iterator<Item = usize>,
        require_whitespace_before: bool,
    ) -> Option<usize> {
        candidates.find(|&candidate| {
            (!require_whitespace_before || self.has_whitespace_before(candidate))
                && self.try_consume_inline_image_end(candidate)
        })
    }

    /// Returns whether the byte immediately before `position` is PDF whitespace.
    ///
    /// This boundary check intentionally narrows `EI` candidates so embedded `EI`
    /// sequences in compressed or binary payloads are less likely to terminate data.
    fn has_whitespace_before(&self, position: usize) -> bool {
        position
            .checked_sub(1)
            .and_then(|index| self.tokenizer.input.get(index))
            .copied()
            .is_some_and(Self::is_pdf_whitespace)
    }

    /// Attempts to consume `EI` as a parser keyword at `candidate_start`.
    ///
    /// Delegating to `read_keyword` centralizes delimiter validation and keeps inline
    /// image termination rules aligned with parser keyword semantics.
    fn try_consume_inline_image_end(&mut self, candidate_start: usize) -> bool {
        self.tokenizer.position = candidate_start;
        self.read_keyword(INLINE_IMAGE_DATA_END).is_ok()
    }

    /// Extracts inline-image raw bytes in `[data_start, data_end)`.
    ///
    /// The range is half-open by design: `data_end` points at the first `E` of `EI`,
    /// which must not be included in image payload bytes.
    fn inline_image_data(&self, data_start: usize, data_end: usize) -> Vec<u8> {
        self.tokenizer
            .input
            .get(data_start..data_end)
            .unwrap_or(&[])
            .to_vec()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    /// Parses an inline image's dictionary and data, given where its data ends.
    fn parse_inline_image(
        parser: &mut PdfParser<'_>,
        end: InlineImageDataEnd,
    ) -> Result<Vec<u8>, ParserError> {
        parser.parse_inline_image_dictionary()?;
        parser.parse_inline_image_data(end)
    }

    #[test]
    fn rejects_non_whitespace_after_id() {
        let mut parser = PdfParser::from(b"/W 1 /H 1 ID/abc EI".as_slice());

        let error = parser.parse_inline_image_dictionary().unwrap_err();
        assert!(matches!(
            error,
            ParserError::InlineImageMissingDataSeparator {
                found: b'/',
                position: _,
            }
        ));
    }

    #[test]
    fn returns_specific_error_when_ei_is_missing() {
        let mut parser = PdfParser::from(b"/W 1 /H 1 ID abc".as_slice());

        let error = parse_inline_image(&mut parser, InlineImageDataEnd::Unknown).unwrap_err();
        assert_eq!(error, ParserError::InlineImageMissingDataEnd);
    }

    #[test]
    fn ignores_embedded_ei_without_whitespace_boundary() {
        let mut parser = PdfParser::from(b"/W 1 /H 1 /BPC 8 /CS /G ID abcEIxdef\nEI Q".as_slice());

        let data = parse_inline_image(&mut parser, InlineImageDataEnd::Length(1)).unwrap();

        assert_eq!(data, b"abcEIxdef\n");
        assert_eq!(parser.tokenizer.data(), b" Q");
    }

    #[test]
    fn parses_exact_length_inline_image_without_whitespace_before_ei() {
        let mut input = b"/W 16 /H 17 /BPC 1 /IM true ID ".to_vec();
        input.extend(std::iter::repeat_n(0xFF, 34));
        input.extend_from_slice(b"EI Q");

        let mut parser = PdfParser::from(input.as_slice());
        let data = parse_inline_image(&mut parser, InlineImageDataEnd::Length(34)).unwrap();

        assert_eq!(data.len(), 34);
        assert!(data.iter().all(|byte| *byte == 0xFF));
        assert_eq!(parser.tokenizer.data(), b" Q");
    }

    #[test]
    fn exact_length_inline_image_ignores_embedded_ei_before_true_end() {
        let mut parser =
            PdfParser::from(b"/W 10 /H 1 /BPC 8 /CS /DeviceGray ID abc EIxyzjEI Q".as_slice());

        let data = parse_inline_image(&mut parser, InlineImageDataEnd::Length(10)).unwrap();

        assert_eq!(data, b"abc EIxyzj");
        assert_eq!(parser.tokenizer.data(), b" Q");
    }

    #[test]
    fn exact_length_inline_image_allows_whitespace_before_ei() {
        let mut parser = PdfParser::from(b"/W 1 /H 1 /BPC 1 /IM true ID \x00\nEI Q".as_slice());

        let data = parse_inline_image(&mut parser, InlineImageDataEnd::Length(1)).unwrap();

        assert_eq!(data, b"\x00\n");
        assert_eq!(parser.tokenizer.data(), b" Q");
    }

    #[test]
    fn exact_length_inline_image_allows_newline_immediately_after_id() {
        let mut input = b"/CS /G /I true /W 2 /H 1 /BPC 8 ID\n".to_vec();
        input.extend_from_slice(&[0xFF, 0x80]);
        input.extend_from_slice(b"\nEI Q");

        let mut parser = PdfParser::from(input.as_slice());
        let data = parse_inline_image(&mut parser, InlineImageDataEnd::Length(2)).unwrap();

        assert_eq!(data, &[0xFF, 0x80, b'\n']);
        assert_eq!(parser.tokenizer.data(), b" Q");
    }

    #[test]
    fn filtered_inline_image_scans_to_the_terminator_before_filtering() {
        let mut parser = PdfParser::from(
            b"/W 1 /H 1 /BPC 8 /CS /G /F /ASCIIHexDecode ID aa EIxyz\nEI Q".as_slice(),
        );

        let data = parse_inline_image(&mut parser, InlineImageDataEnd::Unknown).unwrap();

        assert_eq!(data, b"aa EIxyz\n");
        assert_eq!(parser.tokenizer.data(), b" Q");
    }

    #[test]
    fn reads_data_after_empty_inline_image_dictionary() {
        let mut parser = PdfParser::from(b"ID x\nEI".as_slice());

        let dictionary = parser.parse_inline_image_dictionary().unwrap();
        assert!(dictionary.is_empty());
        assert_eq!(
            parser
                .parse_inline_image_data(InlineImageDataEnd::Unknown)
                .unwrap(),
            b"x\n"
        );
        assert_eq!(parser.tokenizer.data(), b"");
    }

    #[test]
    fn inline_image_metadata_stops_before_id_after_name_value() {
        let mut parser = PdfParser::from(b"/W 1 /H 1 /BPC 8 /CS /DeviceGray ID x\nEI".as_slice());

        let data = parse_inline_image(&mut parser, InlineImageDataEnd::Length(1)).unwrap();

        assert_eq!(data, b"x\n");
        assert_eq!(parser.tokenizer.data(), b"");
    }

    #[test]
    fn filtered_inline_image_accepts_ei_directly_after_binary_data() {
        let mut parser = PdfParser::from(
            b"/IM true /W 8 /H 1 /F /CCF /DP << /K -1 /Columns 8 >> ID \x01}\x10EI\nQ".as_slice(),
        );

        assert!(parse_inline_image(&mut parser, InlineImageDataEnd::Unknown).is_ok());
        assert_eq!(parser.tokenizer.data(), b"Q");
    }

    #[test]
    fn inline_image_dictionary_rejects_trailing_key_without_value() {
        let mut parser = PdfParser::from(b"/Title ID x\nEI".as_slice());

        assert!(parse_inline_image(&mut parser, InlineImageDataEnd::Unknown).is_err());
    }

    #[test]
    fn ascii85_inline_image_ends_at_ei_after_its_end_of_data_marker() {
        let mut parser = PdfParser::from(
            b"/W 1 /H 1 /BPC 8 /CS /G /F /A85 ID 9jqo\r\nEI(^~>\r\nEI Q".as_slice(),
        );

        let data = parse_inline_image(&mut parser, InlineImageDataEnd::Marker(b"~>")).unwrap();
        assert_eq!(data, b"9jqo\r\nEI(^~>\r\n");
        assert_eq!(parser.tokenizer.data(), b" Q");
    }
}
