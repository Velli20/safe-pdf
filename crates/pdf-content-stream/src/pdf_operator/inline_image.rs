use pdf_object::object_resolver::PassthroughResolver;
use pdf_parser::parser::PdfParser;

use crate::{error::PdfOperatorError, xobject_and_image_operators::InlineImage};

use super::variants::PdfOperatorVariant;

const INLINE_IMAGE_DATA_BEGIN: &[u8] = b"ID";
const INLINE_IMAGE_DATA_END: &[u8] = b"EI";

/// Parses a `BI ... ID ... EI` inline-image section and appends it as one operator.
///
/// Inline images are syntactically special in content streams: their dictionary and
/// byte payload are embedded directly in the operator stream. This entry point keeps
/// that complexity isolated from the generic operator parser.
pub(crate) fn parse_inline_image(
    parser: &mut PdfParser<'_>,
    out: &mut Vec<PdfOperatorVariant>,
) -> Result<(), PdfOperatorError> {
    InlineImageReader::new(parser).parse_into(out)
}

struct InlineImageReader<'parser, 'input> {
    parser: &'parser mut PdfParser<'input>,
}

impl<'parser, 'input> InlineImageReader<'parser, 'input> {
    /// Creates a reader bound to the shared content-stream parser state.
    ///
    /// The inline-image parser must mutate the same tokenizer position as the outer
    /// operator parser so parsing can resume at the correct byte after `EI`.
    fn new(parser: &'parser mut PdfParser<'input>) -> Self {
        Self { parser }
    }

    /// Parses the inline-image dictionary and data payload, then materializes
    /// a single `PdfOperatorVariant::InlineImage`.
    ///
    /// The order is fixed by the PDF grammar: dictionary entries end at `ID`,
    /// followed by one required separator byte/EOL, then raw image bytes up to `EI`.
    fn parse_into(mut self, out: &mut Vec<PdfOperatorVariant>) -> Result<(), PdfOperatorError> {
        let dictionary = self
            .parser
            .parse_dictionary_until_keyword(&PassthroughResolver, INLINE_IMAGE_DATA_BEGIN)?;
        self.consume_required_data_separator()?;

        let data = self.read_data_until_end()?;
        out.push(PdfOperatorVariant::InlineImage(InlineImage::new(
            dictionary, data,
        )));

        Ok(())
    }

    /// Consumes the mandatory separator immediately after the `ID` keyword.
    ///
    /// PDF inline-image syntax requires whitespace after `ID` before data begins.
    /// Enforcing this avoids ambiguous parses where data bytes could be mistaken for
    /// dictionary/operator content.
    fn consume_required_data_separator(&mut self) -> Result<(), PdfOperatorError> {
        let Some(first) = self.parser.tokenizer.data().first().copied() else {
            return Err(PdfOperatorError::InlineImageMissingDataEnd);
        };

        if !PdfParser::is_pdf_whitespace(first) {
            return Err(PdfOperatorError::InlineImageMissingDataSeparator {
                found: first,
                position: self.parser.tokenizer.position,
            });
        }

        if matches!(first, b'\r' | b'\n') {
            self.parser.try_read_end_of_line_marker();
        } else {
            let _ = self.parser.tokenizer.read_exactly(1)?;
        }

        Ok(())
    }

    /// Reads raw inline-image bytes until the validated `EI` terminator.
    ///
    /// This first resolves the end position, then slices bytes once. Separating
    /// discovery from extraction keeps state transitions predictable and minimizes
    /// accidental cursor drift.
    fn read_data_until_end(&mut self) -> Result<Vec<u8>, PdfOperatorError> {
        let data_start = self.parser.tokenizer.position;
        let data_end = self.find_inline_image_data_end()?;
        Ok(self.inline_image_data(data_start, data_end))
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
    fn find_inline_image_data_end(&mut self) -> Result<usize, PdfOperatorError> {
        let current = self.parser.tokenizer.position;
        let remaining = self.parser.tokenizer.data();
        let candidates: Vec<usize> = remaining
            .iter()
            .enumerate()
            .filter_map(|(offset, byte)| (*byte == b'E').then_some(current.saturating_add(offset)))
            .collect();

        candidates
            .into_iter()
            .find_map(|candidate| {
                if !self.has_whitespace_before(candidate) {
                    return None;
                }
                self.try_consume_inline_image_end(candidate)
                    .then_some(candidate)
            })
            .ok_or(PdfOperatorError::InlineImageMissingDataEnd)
    }

    /// Returns whether the byte immediately before `position` is PDF whitespace.
    ///
    /// This boundary check intentionally narrows `EI` candidates so embedded `EI`
    /// sequences in compressed/binary payloads are less likely to terminate data.
    fn has_whitespace_before(&self, position: usize) -> bool {
        position
            .checked_sub(1)
            .and_then(|index| self.parser.tokenizer.input.get(index))
            .copied()
            .is_some_and(PdfParser::is_pdf_whitespace)
    }

    /// Attempts to consume `EI` as a parser keyword at `candidate_start`.
    ///
    /// Delegating to `read_keyword` centralizes delimiter validation and keeps inline
    /// image termination rules aligned with parser keyword semantics.
    fn try_consume_inline_image_end(&mut self, candidate_start: usize) -> bool {
        self.parser.tokenizer.position = candidate_start;
        self.parser.read_keyword(INLINE_IMAGE_DATA_END).is_ok()
    }

    /// Extracts inline-image raw bytes in `[data_start, data_end)`.
    ///
    /// The range is half-open by design: `data_end` points at the first `E` of `EI`,
    /// which must not be included in image payload bytes.
    fn inline_image_data(&self, data_start: usize, data_end: usize) -> Vec<u8> {
        self.parser
            .tokenizer
            .input
            .get(data_start..data_end)
            .unwrap_or(&[])
            .to_vec()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use pdf_object::object_resolver::PassthroughResolver;

    use super::*;

    #[test]
    fn rejects_non_whitespace_after_id() {
        let mut parser = PdfParser::from(b"/W 1 /H 1 ID/abc EI".as_slice());
        let mut out = Vec::new();

        let error = parse_inline_image(&mut parser, &mut out).unwrap_err();
        assert!(matches!(
            error,
            PdfOperatorError::InlineImageMissingDataSeparator {
                found: b'/',
                position: _,
            }
        ));
    }

    #[test]
    fn returns_specific_error_when_ei_is_missing() {
        let mut parser = PdfParser::from(b"/W 1 /H 1 ID abc".as_slice());
        let mut out = Vec::new();

        let error = parse_inline_image(&mut parser, &mut out).unwrap_err();
        assert_eq!(error, PdfOperatorError::InlineImageMissingDataEnd);
    }

    #[test]
    fn ignores_embedded_ei_without_whitespace_boundary() {
        let mut parser = PdfParser::from(b"/W 1 /H 1 ID abcEIxdef\nEI Q".as_slice());
        let mut out = Vec::new();

        parse_inline_image(&mut parser, &mut out).unwrap();
        assert!(matches!(out.first(), Some(PdfOperatorVariant::InlineImage(_))));
        let image = out.first().and_then(|operator| match operator {
            PdfOperatorVariant::InlineImage(image) => Some(image),
            _ => None,
        });

        assert_eq!(image.map(InlineImage::data), Some(b"abcEIxdef\n".as_slice()));
        assert_eq!(parser.tokenizer.data(), b" Q");
    }

    #[test]
    fn parse_dictionary_until_keyword_still_consumes_id() {
        let mut parser = PdfParser::from(b"/W 1 /H 1 ID \x00\x01EI".as_slice());
        let dictionary = parser
            .parse_dictionary_until_keyword(&PassthroughResolver, INLINE_IMAGE_DATA_BEGIN)
            .unwrap();

        assert_eq!(dictionary.dictionary.len(), 2);
        assert_eq!(parser.tokenizer.data(), b" \x00\x01EI");
    }
}
