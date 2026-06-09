//! Per-segment JBIG2 decode context.

use crate::{
    arith_decoder::JBig2ArithDecoder,
    error::Jbig2Error,
    image::JBig2Image,
    pattern_dictionary::PatternDictionary,
    segment::{JBig2SegmentResult, ParsedSegment},
};
use pdf_utils::BitReader;

/// State shared by decoders while processing one JBIG2 segment body.
///
/// Segment dispatch owns the current [`ParsedSegment`], the shared input
/// reader, the byte position where the current segment ends, and the decoded
/// segments that appeared earlier in the current stream plus any prior/global
/// segments supplied by the caller. This context keeps those orchestration
/// details at the segment boundary so individual decoders can request bounded
/// body data and resolved references without threading raw `segment_end` and
/// slice lookups through their call stacks.
pub(crate) struct SegmentDecodeContext<'segment, 'stream, 'data, 'decoded, 'prior> {
    segment: &'segment ParsedSegment,
    stream: &'stream mut BitReader<'data>,
    segment_end: usize,
    segments: &'decoded [ParsedSegment],
    prior_segments: &'prior [ParsedSegment],
}

impl<'segment, 'stream, 'data, 'decoded, 'prior>
    SegmentDecodeContext<'segment, 'stream, 'data, 'decoded, 'prior>
{
    /// Creates a context for decoding `segment` from `stream`.
    ///
    /// `segment_end` is an absolute byte position in the original input data.
    /// `prior_segments` must contain only segments decoded before `segment`,
    /// matching JBIG2 reference resolution rules for this decoder.
    pub(crate) fn new(
        segment: &'segment ParsedSegment,
        stream: &'stream mut BitReader<'data>,
        segment_end: usize,
        segments: &'decoded [ParsedSegment],
        prior_segments: &'prior [ParsedSegment],
    ) -> Self {
        Self {
            segment,
            stream,
            segment_end,
            segments,
            prior_segments,
        }
    }

    /// Returns mutable access to the shared stream at its current position.
    ///
    /// Segment decoders use this to parse fixed-size headers before asking the
    /// context for the remaining bounded segment body.
    pub(crate) fn stream(&mut self) -> &mut BitReader<'data> {
        self.stream
    }

    /// Returns the absolute byte position where the current segment body ends.
    ///
    /// Prefer [`Self::remaining_body`] or [`Self::arithmetic_decoder_until_end`]
    /// when possible; this accessor exists for parsers that still need to pass
    /// a byte limit into existing lower-level helpers.
    pub(crate) fn segment_end(&self) -> usize {
        self.segment_end
    }

    /// Returns the remaining bytes in the current segment body.
    ///
    /// The slice starts at the stream's current byte position and ends at this
    /// context's segment boundary. `label` is used in the typed truncation
    /// error when the bounded range is not available.
    pub(crate) fn remaining_body(&self, label: &'static str) -> Result<&'data [u8], Jbig2Error> {
        self.stream
            .remaining_from_byte_until(self.segment_end)
            .ok_or(Jbig2Error::Truncated(label))
    }

    /// Creates an arithmetic decoder limited to the current segment boundary.
    ///
    /// This preserves the shared stream position and enforces that arithmetic
    /// reads cannot continue beyond the segment body.
    pub(crate) fn arithmetic_decoder_until_end(
        &mut self,
    ) -> Result<JBig2ArithDecoder<'_, 'data>, Jbig2Error> {
        JBig2ArithDecoder::new_until(self.stream, self.segment_end)
    }

    fn referred_segment(&self, referred_number: u32) -> Option<&ParsedSegment> {
        self.segments
            .iter()
            .find(|candidate| candidate.number == referred_number)
            .or_else(|| {
                self.prior_segments
                    .iter()
                    .find(|candidate| candidate.number == referred_number)
            })
    }

    /// Collects symbol images from symbol dictionary segments referenced by the current segment.
    ///
    /// Missing referenced segment numbers return [`Jbig2Error::MissingSegment`].
    /// Referenced segments that are present but are not symbol dictionaries are
    /// ignored, matching the previous JBIG2 symbol lookup behavior.
    pub(crate) fn referred_symbol_images(&self) -> Result<Vec<JBig2Image>, Jbig2Error> {
        let mut images = Vec::new();
        for referred_number in &self.segment.referred_to_segment_numbers {
            let referred = self
                .referred_segment(*referred_number)
                .ok_or(Jbig2Error::MissingSegment)?;
            if let JBig2SegmentResult::SymbolDictionary(dict) = &referred.result {
                images.extend(dict.images.iter().cloned());
            }
        }
        Ok(images)
    }

    /// Resolves the first pattern dictionary referenced by the current segment.
    ///
    /// Missing referred segment numbers are skipped so later references can
    /// still resolve, preserving the halftone decoder's prior lookup behavior.
    /// Returns [`Jbig2Error::MissingSegment`] when no referenced pattern
    /// dictionary is available.
    pub(crate) fn referred_pattern_dictionary(&self) -> Result<&PatternDictionary, Jbig2Error> {
        for referred_number in &self.segment.referred_to_segment_numbers {
            let Some(referred) = self.referred_segment(*referred_number) else {
                continue;
            };
            if let JBig2SegmentResult::PatternDictionary(dict) = &referred.result {
                return Ok(dict);
            }
        }
        Err(Jbig2Error::MissingSegment)
    }

    /// Resolves the first bitmap image referenced by the current segment.
    ///
    /// Generic refinement regions refine an already-decoded bitmap segment.
    /// Missing referred segment numbers are skipped so later references can
    /// still resolve, matching pattern-dictionary lookup behavior.
    pub(crate) fn referred_image(&self) -> Result<&JBig2Image, Jbig2Error> {
        for referred_number in &self.segment.referred_to_segment_numbers {
            let Some(referred) = self.referred_segment(*referred_number) else {
                continue;
            };
            if let JBig2SegmentResult::Image(image) = &referred.result {
                return Ok(image);
            }
        }
        Err(Jbig2Error::MissingSegment)
    }

    /// Resolve the first referenced bitmap, or use `fallback` when the segment
    /// has no explicit references.
    pub(crate) fn referred_image_or<'fallback>(
        &'fallback self,
        fallback: Option<&'fallback JBig2Image>,
    ) -> Result<&'fallback JBig2Image, Jbig2Error> {
        if self.segment.referred_to_segment_numbers.is_empty()
            && let Some(image) = fallback
        {
            return Ok(image);
        }

        self.referred_image()
    }
}

#[cfg(test)]
mod tests {
    use super::SegmentDecodeContext;
    use pdf_utils::BitReader;

    use crate::{
        error::Jbig2Error,
        image::JBig2Image,
        pattern_dictionary::PatternDictionary,
        segment::{JBig2SegmentResult, ParsedSegment},
        symbol_dictionary::SymbolDictionary,
    };

    #[test]
    fn missing_referred_symbol_segment_fails_cleanly() {
        let segment = ParsedSegment {
            number: 2,
            flags: 0,
            referred_to_segment_numbers: vec![1],
            page_association: 0,
            data_length: None,
            result: JBig2SegmentResult::None,
        };
        let mut stream = BitReader::new(&[]);
        let context = SegmentDecodeContext::new(&segment, &mut stream, 0, &[], &[]);

        let err = context.referred_symbol_images().expect_err("error");

        assert_eq!(err, Jbig2Error::MissingSegment);
    }

    #[test]
    fn referred_symbol_dictionary_images_are_collected() {
        let symbol = JBig2Image::new(1, 1);
        let referred = ParsedSegment {
            number: 1,
            flags: 0,
            referred_to_segment_numbers: vec![],
            page_association: 0,
            data_length: None,
            result: JBig2SegmentResult::SymbolDictionary(SymbolDictionary {
                images: vec![symbol],
            }),
        };
        let segment = ParsedSegment {
            number: 2,
            flags: 0,
            referred_to_segment_numbers: vec![1],
            page_association: 0,
            data_length: None,
            result: JBig2SegmentResult::None,
        };
        let mut stream = BitReader::new(&[]);
        let prior_segments = [referred];
        let context = SegmentDecodeContext::new(&segment, &mut stream, 0, &[], &prior_segments);

        let images = context.referred_symbol_images().expect("images");

        assert_eq!(images.len(), 1);
    }

    #[test]
    fn referred_symbol_dictionary_images_are_collected_from_current_and_prior_segments() {
        let current_referred = ParsedSegment {
            number: 1,
            flags: 0,
            referred_to_segment_numbers: vec![],
            page_association: 0,
            data_length: None,
            result: JBig2SegmentResult::SymbolDictionary(SymbolDictionary {
                images: vec![JBig2Image::new(1, 1)],
            }),
        };
        let prior_referred = ParsedSegment {
            number: 2,
            flags: 0,
            referred_to_segment_numbers: vec![],
            page_association: 0,
            data_length: None,
            result: JBig2SegmentResult::SymbolDictionary(SymbolDictionary {
                images: vec![JBig2Image::new(2, 1)],
            }),
        };
        let segment = ParsedSegment {
            number: 3,
            flags: 0,
            referred_to_segment_numbers: vec![1, 2],
            page_association: 0,
            data_length: None,
            result: JBig2SegmentResult::None,
        };
        let mut stream = BitReader::new(&[]);
        let current_segments = [current_referred];
        let prior_segments = [prior_referred];
        let context =
            SegmentDecodeContext::new(&segment, &mut stream, 0, &current_segments, &prior_segments);

        let images = context.referred_symbol_images().expect("images");

        assert_eq!(images.len(), 2);
        assert_eq!(images.first().expect("first").width(), 1);
        assert_eq!(images.get(1).expect("second").width(), 2);
    }

    #[test]
    fn referred_pattern_dictionary_is_resolved() {
        let referred = ParsedSegment {
            number: 1,
            flags: 0,
            referred_to_segment_numbers: vec![],
            page_association: 0,
            data_length: None,
            result: JBig2SegmentResult::PatternDictionary(PatternDictionary {
                pattern_width: 1,
                pattern_height: 1,
                patterns: vec![],
            }),
        };
        let segment = ParsedSegment {
            number: 2,
            flags: 0,
            referred_to_segment_numbers: vec![1],
            page_association: 0,
            data_length: None,
            result: JBig2SegmentResult::None,
        };
        let mut stream = BitReader::new(&[]);
        let prior_segments = [referred];
        let context = SegmentDecodeContext::new(&segment, &mut stream, 0, &[], &prior_segments);

        let dict = context.referred_pattern_dictionary().expect("dictionary");

        assert_eq!(dict.pattern_width, 1);
    }
}
