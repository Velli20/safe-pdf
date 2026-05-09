use pdf_image::InlineImage;
use pdf_object::{dictionary::Dictionary, object_resolver::ObjectResolver};

use crate::{error::ParserError, parser::PdfParser};

const INLINE_IMAGE_DATA_BEGIN: &[u8] = b"ID";
const INLINE_IMAGE_DATA_END: &[u8] = b"EI";

impl PdfParser<'_> {
    /// Parses an inline image after the `BI` operator has already been consumed.
    ///
    /// Returns the canonical `pdf_image::InlineImage` representation.
    pub fn parse_inline_image(
        &mut self,
        objects: &dyn ObjectResolver,
    ) -> Result<InlineImage, ParserError> {
        let dictionary = self.parse_dictionary_until_keyword(objects, INLINE_IMAGE_DATA_BEGIN)?;
        self.consume_inline_image_data_separator()?;
        let data = self.read_inline_image_data_until_end(&dictionary, objects)?;
        Ok(InlineImage::new(dictionary, data))
    }

    /// Consumes the mandatory separator immediately after the `ID` keyword.
    ///
    /// PDF inline-image syntax requires whitespace after `ID` before data begins.
    /// Enforcing this avoids ambiguous parses where data bytes could be mistaken for
    /// dictionary or operator content.
    fn consume_inline_image_data_separator(&mut self) -> Result<(), ParserError> {
        let Some(first) = self.tokenizer.data().first().copied() else {
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

    /// Reads raw inline-image bytes until the validated `EI` terminator.
    ///
    /// This first resolves the end position, then slices bytes once. Separating
    /// discovery from extraction keeps state transitions predictable and minimizes
    /// accidental cursor drift.
    fn read_inline_image_data_until_end(
        &mut self,
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Vec<u8>, ParserError> {
        let data_start = self.tokenizer.position;
        let data_end = self.find_inline_image_data_end(data_start, dictionary, objects)?;
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
    fn find_inline_image_data_end(
        &mut self,
        data_start: usize,
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<usize, ParserError> {
        if let Some(data_end) =
            self.try_find_inline_image_data_end_by_length(data_start, dictionary, objects)?
        {
            return Ok(data_end);
        }

        self.find_inline_image_data_end_by_scan()
    }

    /// Resolves the inline-image end using exact metadata-derived payload length.
    ///
    /// This is only reliable for unfiltered inline images whose dictionary provides
    /// enough information to derive the raw byte size directly.
    fn try_find_inline_image_data_end_by_length(
        &mut self,
        data_start: usize,
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Option<usize>, ParserError> {
        let Some(expected_length) = Self::try_compute_inline_image_data_length(dictionary, objects)
        else {
            return Ok(None);
        };

        let Some(candidate_start) = data_start.checked_add(expected_length) else {
            return Err(ParserError::InlineImageMissingDataEnd);
        };

        if self.try_consume_inline_image_end(candidate_start) {
            return Ok(Some(candidate_start));
        }

        Ok(None)
    }

    /// Finds the first valid inline-image `EI` end marker by scanning binary data.
    ///
    /// The fallback path intentionally keeps the conservative whitespace-before-`EI`
    /// guard so binary or compressed inline-image payloads do not terminate early on
    /// embedded `EI` byte sequences.
    fn find_inline_image_data_end_by_scan(&mut self) -> Result<usize, ParserError> {
        let current = self.tokenizer.position;
        let candidates: Vec<usize> = self
            .tokenizer
            .data()
            .iter()
            .enumerate()
            .filter_map(|(offset, byte)| (*byte == b'E').then_some(current.saturating_add(offset)))
            .collect();

        for candidate in candidates {
            if !self.has_whitespace_before(candidate) {
                continue;
            }

            if self.try_consume_inline_image_end(candidate) {
                return Ok(candidate);
            }
        }

        Err(ParserError::InlineImageMissingDataEnd)
    }

    /// Computes the raw payload length for an unfiltered inline image when possible.
    fn try_compute_inline_image_data_length(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Option<usize> {
        if dictionary.get("F").is_some() || dictionary.get("Filter").is_some() {
            return None;
        }

        let width = Self::try_inline_image_number(dictionary, "W", "Width", objects)?;
        let height = Self::try_inline_image_number(dictionary, "H", "Height", objects)?;
        let bits_per_component =
            Self::try_inline_image_number(dictionary, "BPC", "BitsPerComponent", objects)?;
        let samples_per_pixel = Self::try_inline_image_samples_per_pixel(dictionary, objects)?;

        let bits_per_row = width
            .checked_mul(samples_per_pixel)?
            .checked_mul(bits_per_component)?;
        let bytes_per_row = bits_per_row.div_ceil(8);
        height.checked_mul(bytes_per_row)
    }

    fn try_inline_image_number(
        dictionary: &Dictionary,
        short_key: &str,
        long_key: &str,
        objects: &dyn ObjectResolver,
    ) -> Option<usize> {
        dictionary
            .get(short_key)
            .or_else(|| dictionary.get(long_key))
            .and_then(|value| value.try_number::<usize>(objects).ok())
    }

    fn try_inline_image_samples_per_pixel(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Option<usize> {
        let image_mask = dictionary
            .get("IM")
            .or_else(|| dictionary.get("ImageMask"))
            .and_then(|value| value.try_boolean(objects).ok())
            .unwrap_or(false);
        if image_mask {
            return Some(1);
        }

        let color_space = dictionary
            .get("CS")
            .or_else(|| dictionary.get("ColorSpace"))?;
        let color_space_name = color_space.try_str(objects).ok()?;

        match color_space_name.as_ref() {
            "G" | "DeviceGray" => Some(1),
            "RGB" | "DeviceRGB" => Some(3),
            "CMYK" | "DeviceCMYK" => Some(4),
            "I" | "Indexed" => Some(1),
            _ => None,
        }
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
    use pdf_object::object_resolver::PassthroughResolver;

    use super::*;

    #[test]
    fn rejects_non_whitespace_after_id() {
        let mut parser = PdfParser::from(b"/W 1 /H 1 ID/abc EI".as_slice());

        let error = parser.parse_inline_image(&PassthroughResolver).unwrap_err();
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

        let error = parser.parse_inline_image(&PassthroughResolver).unwrap_err();
        assert_eq!(error, ParserError::InlineImageMissingDataEnd);
    }

    #[test]
    fn ignores_embedded_ei_without_whitespace_boundary() {
        let mut parser = PdfParser::from(b"/W 1 /H 1 ID abcEIxdef\nEI Q".as_slice());

        let image = parser.parse_inline_image(&PassthroughResolver).unwrap();

        assert_eq!(image.data(), b"abcEIxdef\n");
        assert_eq!(parser.tokenizer.data(), b" Q");
    }

    #[test]
    fn parses_exact_length_inline_image_without_whitespace_before_ei() {
        let mut input = b"/W 16 /H 17 /BPC 1 /IM true ID ".to_vec();
        input.extend(std::iter::repeat_n(0xFF, 34));
        input.extend_from_slice(b"EI Q");

        let mut parser = PdfParser::from(input.as_slice());
        let image = parser.parse_inline_image(&PassthroughResolver).unwrap();

        assert_eq!(image.data().len(), 34);
        assert!(image.data().iter().all(|byte| *byte == 0xFF));
        assert_eq!(parser.tokenizer.data(), b" Q");
    }

    #[test]
    fn exact_length_inline_image_ignores_embedded_ei_before_true_end() {
        let mut parser =
            PdfParser::from(b"/W 10 /H 1 /BPC 8 /CS /DeviceGray ID abc EIxyzjEI Q".as_slice());

        let image = parser.parse_inline_image(&PassthroughResolver).unwrap();

        assert_eq!(image.data(), b"abc EIxyzj");
        assert_eq!(parser.tokenizer.data(), b" Q");
    }

    #[test]
    fn exact_length_inline_image_allows_whitespace_before_ei() {
        let mut parser = PdfParser::from(b"/W 1 /H 1 /BPC 1 /IM true ID \x00\nEI Q".as_slice());

        let image = parser.parse_inline_image(&PassthroughResolver).unwrap();

        assert_eq!(image.data(), b"\x00\n");
        assert_eq!(parser.tokenizer.data(), b" Q");
    }

    #[test]
    fn filtered_inline_image_still_uses_scan_terminator_detection() {
        let mut parser =
            PdfParser::from(b"/W 1 /H 1 /BPC 8 /F /ASCIIHexDecode ID aa EIxyz\nEI Q".as_slice());

        let image = parser.parse_inline_image(&PassthroughResolver).unwrap();

        assert_eq!(image.data(), b"aa EIxyz\n");
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
