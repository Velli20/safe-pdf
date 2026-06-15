//! JBIG2 segment-walk orchestration.

use crate::{
    compose_op::ComposeOp,
    error::Jbig2Error,
    generic_refinement_region::decode_generic_refinement_region_segment,
    generic_region::GenericRegion,
    halftone_region::decode_halftone_region_segment,
    huffman::CustomHuffmanDecoder,
    image::JBig2Image,
    page::PageInfo,
    pattern_dictionary::PatternDictionary,
    segment::{JBig2SegmentResult, ParsedSegment, SegmentType},
    segment_context::SegmentDecodeContext,
    symbol_dictionary::SymbolDictionary,
    text_region::decode_text_region_segment,
};
use pdf_utils::BitReader;

use super::decode::DecodedJbig2;

const MIN_SEGMENT_HEADER_BYTES: usize = 11;
const EMPTY_OR_TRUNCATED_STREAM: &str = "empty or truncated stream";
const SEGMENT_DATA: &str = "segment data";
const SEGMENT_DATA_LENGTH_OVERFLOW: &str = "segment data length overflow";
const GENERIC_REGION_DATA: &str = "generic region data";

/// Decode a standalone JBIG2 segment stream.
///
/// JBIG2 T.88 / ISO/IEC 14492 section 7.2 defines files and streams as an
/// ordered sequence of segments. PDF JBIG2 image data can supply page
/// dimensions outside the JBIG2 byte stream, so `page_dims` optionally seeds
/// the output page before segment processing begins.
pub(crate) fn decode_segments(
    data: &[u8],
    page_dims: Option<(u16, u16)>,
) -> Result<DecodedJbig2, Jbig2Error> {
    decode_segments_with_prior(data, page_dims, &[])
}

/// Decode a JBIG2 segment stream after already-decoded global segments.
///
/// PDF JBIG2 streams can reference a separate JBIG2Globals stream. Those prior
/// segments participate in the normal JBIG2 reference rules from T.88 section
/// 7.2.4, so this function preserves them before walking `data`.
pub(crate) fn decode_segments_with_prior(
    data: &[u8],
    page_dims: Option<(u16, u16)>,
    prior_segments: &[ParsedSegment],
) -> Result<DecodedJbig2, Jbig2Error> {
    Jbig2SegmentStreamDecoder::new(data, page_dims, prior_segments)?.decode()
}

/// Stateful walker for a JBIG2 segment stream.
///
/// T.88 / ISO/IEC 14492 section 7.2 describes JBIG2 data as a sequence of
/// segments whose headers identify segment type, references, page association,
/// and body length. This type centralizes the stream cursor, decoded segment
/// table, and page bitmap so individual helper methods can be tested around
/// one responsibility at a time.
struct Jbig2SegmentStreamDecoder<'data, 'prior> {
    data: &'data [u8],
    stream: BitReader<'data>,
    page: JBig2Image,
    segments: Vec<ParsedSegment>,
    prior_segments: &'prior [ParsedSegment],
    saw_segment: bool,
}

impl<'data, 'prior> Jbig2SegmentStreamDecoder<'data, 'prior> {
    /// Create a decoder for a JBIG2 segment sequence.
    ///
    /// T.88 section 7.4.8 lets a Page Information segment define page size and
    /// default pixel value. PDF image streams can also provide dimensions
    /// externally; when `page_dims` is present, those dimensions initialize a
    /// white page before any JBIG2 Page Information segment is encountered.
    fn new(
        data: &'data [u8],
        page_dims: Option<(u16, u16)>,
        prior_segments: &'prior [ParsedSegment],
    ) -> Result<Self, Jbig2Error> {
        Ok(Self {
            data,
            stream: BitReader::new(data),
            page: Self::initial_page(page_dims)?,
            segments: Vec::new(),
            prior_segments,
            saw_segment: false,
        })
    }

    /// Walk all available complete JBIG2 segment headers and bodies.
    ///
    /// T.88 section 7.2.2 requires each segment to start with a segment header.
    /// A stream with no complete header is treated as truncated, matching the
    /// existing decoder behavior for empty or partial data.
    fn decode(mut self) -> Result<DecodedJbig2, Jbig2Error> {
        while self.stream.remaining_bytes() >= MIN_SEGMENT_HEADER_BYTES {
            self.saw_segment = true;
            if !self.decode_next_segment()? {
                break;
            }
        }

        if !self.saw_segment {
            return Err(Jbig2Error::Truncated(EMPTY_OR_TRUNCATED_STREAM));
        }

        Ok(DecodedJbig2 {
            page: self.page,
            segments: self.segments,
        })
    }

    /// Decode one JBIG2 segment and return whether stream walking should continue.
    ///
    /// T.88 section 7.2.3 defines segment type codes, including End of Page and
    /// End of File. Terminal segments stop the walk without being added to the
    /// retained segment table, preserving the historical decoder contract.
    fn decode_next_segment(&mut self) -> Result<bool, Jbig2Error> {
        let mut segment = ParsedSegment::try_from(&mut self.stream)?;
        let segment_end = self.segment_data_end(&segment)?;
        let segment_type = self.known_segment_type(&segment)?;

        if !self.decode_segment_body(segment_type, segment_end, &mut segment)? {
            return Ok(false);
        }

        self.segments.push(segment);
        self.stream.set_byte_pos_preserving_offset(segment_end);
        Ok(true)
    }

    /// Resolve the absolute byte end of the current JBIG2 segment body.
    ///
    /// T.88 section 7.2.7 permits either an explicit segment data length or an
    /// unknown length marker. The decoder maps unknown length to the remaining
    /// input bytes and validates explicit lengths before body dispatch.
    fn segment_data_end(&self, segment: &ParsedSegment) -> Result<usize, Jbig2Error> {
        let data_len = segment
            .data_length
            .unwrap_or_else(|| self.stream.remaining_bytes());
        let end = self
            .stream
            .byte_pos()
            .checked_add(data_len)
            .ok_or(Jbig2Error::Overflow(SEGMENT_DATA_LENGTH_OVERFLOW))?;
        if end > self.data.len() {
            return Err(Jbig2Error::Truncated(SEGMENT_DATA));
        }

        Ok(end)
    }

    /// Convert a raw JBIG2 segment type code into a supported segment variant.
    ///
    /// Segment type codes are defined by T.88 section 7.2.3. Values reserved by
    /// the specification are rejected before any body bytes are consumed.
    fn known_segment_type(&self, segment: &ParsedSegment) -> Result<SegmentType, Jbig2Error> {
        segment
            .segment_type()
            .ok_or_else(|| Jbig2Error::UnsupportedSegmentType(segment.flags_type()))
    }

    /// Dispatch the current segment body according to its JBIG2 segment type.
    ///
    /// T.88 section 7.2.3 assigns segment types to the concrete syntax defined
    /// by later section 7.4 segment bodies. This method keeps that top-level
    /// routing separate from lower-level dictionary, region, and page parsers.
    fn decode_segment_body(
        &mut self,
        segment_type: SegmentType,
        segment_end: usize,
        segment: &mut ParsedSegment,
    ) -> Result<bool, Jbig2Error> {
        match segment_type {
            SegmentType::SymbolDictionary => self.decode_symbol_dictionary(segment, segment_end)?,
            SegmentType::PatternDictionary => {
                self.decode_pattern_dictionary(segment, segment_end)?
            }
            SegmentType::IntermediateTextRegion => {
                self.decode_intermediate_text_region(segment, segment_end)?;
            }
            SegmentType::ImmediateTextRegion | SegmentType::ImmediateLosslessTextRegion => {
                self.decode_immediate_text_region(segment, segment_end)?;
            }
            SegmentType::IntermediateGenericRegion => {
                self.decode_intermediate_generic_region(segment, segment_end)?;
            }
            SegmentType::ImmediateGenericRegion => {
                self.decode_immediate_generic_region(segment, segment_end)?;
            }
            SegmentType::ImmediateLosslessGenericRegion => {
                self.decode_lossless_generic_region(segment, segment_end)?;
            }
            SegmentType::IntermediateGenericRefinementRegion => {
                self.decode_intermediate_generic_refinement_region(segment, segment_end)?;
            }
            SegmentType::ImmediateGenericRefinementRegion => {
                self.decode_immediate_generic_refinement_region(segment, segment_end)?;
            }
            SegmentType::ImmediateLosslessGenericRefinementRegion => {
                self.decode_lossless_generic_refinement_region(segment, segment_end)?;
            }
            SegmentType::IntermediateHalftoneRegion => {
                self.decode_intermediate_halftone_region(segment, segment_end)?;
            }
            SegmentType::ImmediateHalftoneRegion | SegmentType::ImmediateLosslessHalftoneRegion => {
                self.decode_immediate_halftone_region(segment, segment_end)?;
            }
            SegmentType::PageInformation => self.initialize_page_from_info()?,
            SegmentType::EndOfPage | SegmentType::EndOfFile => return Ok(false),
            SegmentType::CodeTable => self.decode_code_table(segment, segment_end)?,
            SegmentType::EndOfStripe | SegmentType::Profile | SegmentType::Extension => {}
        }

        Ok(true)
    }

    /// Decode an initial page bitmap from PDF-supplied dimensions.
    ///
    /// When no dimensions are supplied, the page remains empty until a JBIG2
    /// Page Information segment from T.88 section 7.4.8 provides dimensions.
    fn initial_page(page_dims: Option<(u16, u16)>) -> Result<JBig2Image, Jbig2Error> {
        if let Some((width, height)) = page_dims {
            JBig2Image::try_new(width, height, Some(false))
        } else {
            Ok(JBig2Image::empty())
        }
    }

    /// Decode a symbol dictionary segment body.
    ///
    /// T.88 section 7.4.2 defines symbol dictionary syntax and exported symbol
    /// images. The decoded dictionary is retained for later segment references.
    fn decode_symbol_dictionary(
        &mut self,
        segment: &mut ParsedSegment,
        segment_end: usize,
    ) -> Result<(), Jbig2Error> {
        let mut context = SegmentDecodeContext::new(
            segment,
            &mut self.stream,
            segment_end,
            &self.segments,
            self.prior_segments,
        );
        let dict = SymbolDictionary::from_reader(&mut context)?;
        segment.result = JBig2SegmentResult::SymbolDictionary(dict);
        Ok(())
    }

    /// Decode a pattern dictionary segment body.
    ///
    /// T.88 section 7.4.4 defines pattern dictionary segments used by halftone
    /// regions. The decoded dictionary is retained for later segment references.
    fn decode_pattern_dictionary(
        &mut self,
        segment: &mut ParsedSegment,
        segment_end: usize,
    ) -> Result<(), Jbig2Error> {
        let mut context = SegmentDecodeContext::new(
            segment,
            &mut self.stream,
            segment_end,
            &self.segments,
            self.prior_segments,
        );
        let dict = PatternDictionary::decode(&mut context)?;
        segment.result = JBig2SegmentResult::PatternDictionary(dict);
        Ok(())
    }

    /// Decode a JBIG2 Tables segment and retain its custom Huffman table.
    fn decode_code_table(
        &mut self,
        segment: &mut ParsedSegment,
        segment_end: usize,
    ) -> Result<(), Jbig2Error> {
        let data = self
            .stream
            .remaining_from_byte_until(segment_end)
            .ok_or(Jbig2Error::Truncated("Huffman code table segment"))?;
        segment.result = JBig2SegmentResult::HuffmanTable(CustomHuffmanDecoder::parse(data)?);
        Ok(())
    }

    /// Decode an intermediate text region segment body.
    ///
    /// T.88 section 7.4.3 defines text region segments. Intermediate regions
    /// produce an image result that later segments can reference.
    fn decode_intermediate_text_region(
        &mut self,
        segment: &mut ParsedSegment,
        segment_end: usize,
    ) -> Result<(), Jbig2Error> {
        let mut context = SegmentDecodeContext::new(
            segment,
            &mut self.stream,
            segment_end,
            &self.segments,
            self.prior_segments,
        );
        let decoded = decode_text_region_segment(&mut context)?;
        segment.result = JBig2SegmentResult::Image(decoded.image);
        Ok(())
    }

    /// Decode and compose an immediate text region segment body.
    ///
    /// T.88 section 7.4.3 defines immediate text regions as page-affecting
    /// segments. Their decoded image is composed into the current page bitmap.
    fn decode_immediate_text_region(
        &mut self,
        segment: &ParsedSegment,
        segment_end: usize,
    ) -> Result<(), Jbig2Error> {
        let mut context = SegmentDecodeContext::new(
            segment,
            &mut self.stream,
            segment_end,
            &self.segments,
            self.prior_segments,
        );
        let decoded = decode_text_region_segment(&mut context)?;
        decoded.compose_to(&mut self.page);
        Ok(())
    }

    /// Decode a generic region segment body to an image.
    fn decode_generic_region_image(
        &mut self,
        segment_end: usize,
    ) -> Result<(GenericRegion, JBig2Image), Jbig2Error> {
        let parsed = GenericRegion::parse(&mut self.stream)?;
        let body = self.generic_region_body(segment_end)?;
        let image = parsed.decode(body)?;
        Ok((parsed, image))
    }

    /// Decode an intermediate generic region and retain its image result.
    fn decode_intermediate_generic_region(
        &mut self,
        segment: &mut ParsedSegment,
        segment_end: usize,
    ) -> Result<(), Jbig2Error> {
        let (_parsed, image) = self.decode_generic_region_image(segment_end)?;
        segment.result = JBig2SegmentResult::Image(image);
        Ok(())
    }

    /// Decode and compose an immediate generic region.
    fn decode_immediate_generic_region(
        &mut self,
        segment: &mut ParsedSegment,
        segment_end: usize,
    ) -> Result<(), Jbig2Error> {
        let (parsed, image) = self.decode_generic_region_image(segment_end)?;
        image.compose_clipped_to(
            &mut self.page,
            i32::from(parsed.region.x),
            i32::from(parsed.region.y),
            ComposeOp::from(parsed.region.flags),
        );
        segment.result = JBig2SegmentResult::Image(image);
        Ok(())
    }

    /// Decode a lossless generic region and retain its image result.
    ///
    /// T.88 section 7.4.6 defines immediate lossless generic regions. This
    /// decoder stores the decoded bitmap as the segment result.
    fn decode_lossless_generic_region(
        &mut self,
        segment: &mut ParsedSegment,
        segment_end: usize,
    ) -> Result<(), Jbig2Error> {
        let parsed = GenericRegion::parse(&mut self.stream)?;
        let body = self.generic_region_body(segment_end)?;
        segment.result = JBig2SegmentResult::Image(parsed.decode(body)?);
        Ok(())
    }

    /// Return the remaining bytes in a generic-region segment body.
    ///
    /// T.88 section 7.4.6 places the generic-region bitmap coding data after
    /// the generic-region header fields. `segment_end` bounds that coding data.
    fn generic_region_body(&self, segment_end: usize) -> Result<&'data [u8], Jbig2Error> {
        self.stream
            .remaining_from_byte_until(segment_end)
            .ok_or(Jbig2Error::Truncated(GENERIC_REGION_DATA))
    }

    /// Decode and compose an immediate halftone region segment body.
    ///
    /// T.88 section 7.4.5 defines halftone regions and their pattern dictionary
    /// references. Immediate halftone regions affect the current page bitmap.
    fn decode_immediate_halftone_region(
        &mut self,
        segment: &ParsedSegment,
        segment_end: usize,
    ) -> Result<(), Jbig2Error> {
        let mut context = SegmentDecodeContext::new(
            segment,
            &mut self.stream,
            segment_end,
            &self.segments,
            self.prior_segments,
        );
        let decoded = decode_halftone_region_segment(&mut context)?;
        decoded.compose_clipped_to(&mut self.page);
        Ok(())
    }

    /// Decode an intermediate halftone region and retain its image result.
    ///
    /// T.88 section 7.4.5 defines intermediate halftone regions as decoded
    /// bitmaps that later segments can reference without page composition.
    fn decode_intermediate_halftone_region(
        &mut self,
        segment: &mut ParsedSegment,
        segment_end: usize,
    ) -> Result<(), Jbig2Error> {
        let mut context = SegmentDecodeContext::new(
            segment,
            &mut self.stream,
            segment_end,
            &self.segments,
            self.prior_segments,
        );
        let decoded = decode_halftone_region_segment(&mut context)?;
        segment.result = JBig2SegmentResult::Image(decoded.image);
        Ok(())
    }

    /// Decode an intermediate generic refinement region and retain its image.
    fn decode_intermediate_generic_refinement_region(
        &mut self,
        segment: &mut ParsedSegment,
        segment_end: usize,
    ) -> Result<(), Jbig2Error> {
        let mut context = SegmentDecodeContext::new(
            segment,
            &mut self.stream,
            segment_end,
            &self.segments,
            self.prior_segments,
        );
        let page = self.page.clone();
        let decoded = decode_generic_refinement_region_segment(&mut context, Some(&page))?;
        segment.result = JBig2SegmentResult::Image(decoded.image);
        Ok(())
    }

    /// Decode and compose an immediate generic refinement region.
    fn decode_immediate_generic_refinement_region(
        &mut self,
        segment: &ParsedSegment,
        segment_end: usize,
    ) -> Result<(), Jbig2Error> {
        let mut context = SegmentDecodeContext::new(
            segment,
            &mut self.stream,
            segment_end,
            &self.segments,
            self.prior_segments,
        );
        let page = self.page.clone();
        let decoded = decode_generic_refinement_region_segment(&mut context, Some(&page))?;
        decoded.compose_clipped_to(&mut self.page);
        Ok(())
    }

    /// Decode an immediate lossless generic refinement region and retain its image.
    fn decode_lossless_generic_refinement_region(
        &mut self,
        segment: &mut ParsedSegment,
        segment_end: usize,
    ) -> Result<(), Jbig2Error> {
        let mut context = SegmentDecodeContext::new(
            segment,
            &mut self.stream,
            segment_end,
            &self.segments,
            self.prior_segments,
        );
        let page = self.page.clone();
        let decoded = decode_generic_refinement_region_segment(&mut context, Some(&page))?;
        segment.result = JBig2SegmentResult::Image(decoded.image);
        Ok(())
    }

    /// Apply a JBIG2 Page Information segment to an empty page.
    ///
    /// T.88 section 7.4.8 defines page dimensions and the default pixel value.
    /// PDF-supplied dimensions take precedence because they initialize a
    /// non-empty page before the segment walk reaches Page Information.
    fn initialize_page_from_info(&mut self) -> Result<(), Jbig2Error> {
        let info = PageInfo::parse(&mut self.stream)?;
        if self.page.width() == 0 || self.page.height() == 0 {
            self.page =
                JBig2Image::try_new(info.width, info.height, Some(info.default_pixel_value))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{decode_segments, decode_segments_with_prior};
    use crate::{
        huffman::{
            HuffmanValue, STANDARD_TABLE_B1, STANDARD_TABLE_B2, STANDARD_TABLE_B4,
            StandardHuffmanDecoder, test_support::bits_for_value,
        },
        image::JBig2Image,
        pattern_dictionary::PatternDictionary,
        segment::{JBig2SegmentResult, ParsedSegment, SegmentType},
        segment_header::UNKNOWN_SEGMENT_DATA_LENGTH,
        symbol_dictionary::SymbolDictionary,
    };

    const PAGE_INFORMATION_DATA_LENGTH: u32 = 19;
    const DEFAULT_TEST_RESOLUTION: u32 = 300;
    const DEFAULT_PIXEL_VALUE_FLAG: u8 = 1 << 2;
    const REFERRED_SEGMENT_COUNT_SHIFT: u8 = 5;
    const SEGMENT_TYPE_MASK: u8 = 0x3f;
    const RESERVED_SEGMENT_TYPE_CODE: u8 = 63;

    fn push_u8(bytes: &mut Vec<u8>, value: u8) {
        bytes.push(value);
    }

    fn push_u16(bytes: &mut Vec<u8>, value: u16) {
        bytes.extend_from_slice(&value.to_be_bytes());
    }

    fn push_u32(bytes: &mut Vec<u8>, value: u32) {
        bytes.extend_from_slice(&value.to_be_bytes());
    }

    fn push_i32(bytes: &mut Vec<u8>, value: i32) {
        bytes.extend_from_slice(&value.to_be_bytes());
    }

    fn append_bits(bits: &mut Vec<bool>, code: u32, codelen: u8) {
        for shift in (0..u32::from(codelen)).rev() {
            bits.push(((code >> shift) & 1) != 0);
        }
    }

    fn append_huffman_value(
        bits: &mut Vec<bool>,
        table: &StandardHuffmanDecoder,
        value: HuffmanValue,
    ) {
        let (code, codelen, extra, extra_len) = bits_for_value(table, value).expect("bits");
        append_bits(bits, code, codelen);
        append_bits(bits, extra, extra_len);
    }

    fn bits_to_bytes(bits: &[bool]) -> Vec<u8> {
        let mut bytes = Vec::new();
        let mut current = 0u8;
        for (index, bit) in bits.iter().copied().enumerate() {
            if bit {
                current |= 1u8 << (7usize.saturating_sub(index % 8));
            }
            if index % 8 == 7 {
                bytes.push(current);
                current = 0;
            }
        }
        if bits.len() % 8 != 0 {
            bytes.push(current);
        }
        bytes
    }

    fn make_segment_header(
        number: u32,
        segment_type: SegmentType,
        referred: &[u8],
        page_association: u8,
        data_length: u32,
    ) -> Vec<u8> {
        let mut bytes = Vec::new();
        push_u32(&mut bytes, number);
        push_u8(&mut bytes, segment_type.code() & SEGMENT_TYPE_MASK);
        let referred_count: u8 = u8::try_from(referred.len()).unwrap_or_default();
        push_u8(&mut bytes, referred_count << REFERRED_SEGMENT_COUNT_SHIFT);
        bytes.extend_from_slice(referred);
        push_u8(&mut bytes, page_association);
        push_u32(&mut bytes, data_length);
        bytes
    }

    fn make_reserved_segment_header(
        number: u32,
        segment_type_code: u8,
        page_association: u8,
        data_length: u32,
    ) -> Vec<u8> {
        let mut bytes = Vec::new();
        push_u32(&mut bytes, number);
        push_u8(&mut bytes, segment_type_code & SEGMENT_TYPE_MASK);
        push_u8(&mut bytes, 0);
        push_u8(&mut bytes, page_association);
        push_u32(&mut bytes, data_length);
        bytes
    }

    fn make_segment_header_with_unknown_data_length(
        number: u32,
        segment_type: SegmentType,
        page_association: u8,
    ) -> Vec<u8> {
        make_segment_header(
            number,
            segment_type,
            &[],
            page_association,
            UNKNOWN_SEGMENT_DATA_LENGTH,
        )
    }

    fn make_page_info_segment(
        number: u32,
        width: u16,
        height: u16,
        default_pixel: bool,
    ) -> Vec<u8> {
        let mut bytes = make_segment_header(
            number,
            SegmentType::PageInformation,
            &[],
            1,
            PAGE_INFORMATION_DATA_LENGTH,
        );
        push_u32(&mut bytes, u32::from(width));
        push_u32(&mut bytes, u32::from(height));
        push_u32(&mut bytes, DEFAULT_TEST_RESOLUTION);
        push_u32(&mut bytes, DEFAULT_TEST_RESOLUTION);
        push_u8(
            &mut bytes,
            if default_pixel {
                DEFAULT_PIXEL_VALUE_FLAG
            } else {
                0x00
            },
        );
        push_u16(&mut bytes, 0x0000);
        bytes
    }

    fn make_minimal_halftone_payload() -> Vec<u8> {
        let mut bytes = Vec::new();
        push_u32(&mut bytes, 1);
        push_u32(&mut bytes, 1);
        push_u32(&mut bytes, 0);
        push_u32(&mut bytes, 0);
        push_u8(&mut bytes, 0);
        push_u8(&mut bytes, 0);
        push_u32(&mut bytes, 1);
        push_u32(&mut bytes, 1);
        push_i32(&mut bytes, 0);
        push_i32(&mut bytes, 0);
        push_u16(&mut bytes, 0);
        push_u16(&mut bytes, 0);
        bytes
    }

    #[test]
    fn empty_or_truncated_stream_returns_error() {
        let err = decode_segments(&[], None).expect_err("error");
        assert_eq!(
            err,
            crate::error::Jbig2Error::Truncated("empty or truncated stream")
        );
    }

    #[test]
    fn unsupported_segment_type_returns_error() {
        let stream = make_reserved_segment_header(1, RESERVED_SEGMENT_TYPE_CODE, 1, 0);
        let err = decode_segments(&stream, None).expect_err("error");
        assert_eq!(
            err,
            crate::error::Jbig2Error::UnsupportedSegmentType(RESERVED_SEGMENT_TYPE_CODE)
        );
    }

    #[test]
    fn declared_segment_length_past_input_returns_truncated_data() {
        let stream = make_reserved_segment_header(1, SegmentType::EndOfStripe.code(), 1, 1);

        let err = decode_segments(&stream, None).expect_err("error");

        assert_eq!(err, crate::error::Jbig2Error::Truncated("segment data"));
    }

    #[test]
    fn prior_segments_are_not_copied_into_current_stream_results() {
        let prior = ParsedSegment {
            number: 10,
            flags: SegmentType::SymbolDictionary.code(),
            referred_to_segment_numbers: vec![],
            page_association: 0,
            data_length: Some(0),
            result: JBig2SegmentResult::SymbolDictionary(SymbolDictionary {
                images: vec![JBig2Image::new(1, 1)],
            }),
        };
        let stream = make_reserved_segment_header(1, SegmentType::EndOfFile.code(), 1, 0);

        let decoded = decode_segments_with_prior(&stream, Some((1, 1)), &[prior]).expect("decode");

        assert!(decoded.segments.is_empty());
    }

    #[test]
    fn page_info_initializes_page_image_correctly() {
        let stream = make_page_info_segment(1, 8, 1, true);
        let decoded = decode_segments(&stream, None).expect("decode");
        assert_eq!(decoded.page.width(), 8);
        assert_eq!(decoded.page.height(), 1);
        assert_eq!(decoded.page.get_pixel(0, 0), 1);
    }

    #[test]
    fn explicit_page_dimensions_initialize_page_before_segments() {
        let stream = make_reserved_segment_header(1, SegmentType::EndOfFile.code(), 1, 0);
        let decoded = decode_segments(&stream, Some((2, 3))).expect("decode");

        assert_eq!(decoded.page.width(), 2);
        assert_eq!(decoded.page.height(), 3);
        assert_eq!(decoded.page.get_pixel(0, 0), 0);
    }

    #[test]
    fn page_info_does_not_replace_explicit_page_dimensions() {
        let stream = make_page_info_segment(1, 8, 1, true);
        let decoded = decode_segments(&stream, Some((2, 2))).expect("decode");

        assert_eq!(decoded.page.width(), 2);
        assert_eq!(decoded.page.height(), 2);
        assert_eq!(decoded.page.get_pixel(0, 0), 0);
    }

    #[test]
    fn unknown_segment_data_length_consumes_remaining_stream() {
        let mut stream =
            make_segment_header_with_unknown_data_length(1, SegmentType::EndOfStripe, 1);
        stream.extend_from_slice(&[0xde, 0xad, 0xbe, 0xef]);

        let decoded = decode_segments(&stream, Some((1, 1))).expect("decode");

        let segment = decoded.segments.first().expect("segment");
        assert_eq!(segment.data_length, None);
        assert_eq!(decoded.segments.len(), 1);
    }

    #[test]
    fn terminal_segments_stop_without_being_retained() {
        let stream = make_reserved_segment_header(1, SegmentType::EndOfFile.code(), 1, 0);
        let decoded = decode_segments(&stream, Some((1, 1))).expect("decode");

        assert!(decoded.segments.is_empty());
    }

    #[test]
    fn intermediate_halftone_region_is_retained_as_image() {
        let mut pattern = JBig2Image::new(1, 1);
        pattern.set_pixel(0, 0, 1);
        let prior = ParsedSegment {
            number: 1,
            flags: SegmentType::PatternDictionary.code(),
            referred_to_segment_numbers: vec![],
            page_association: 0,
            data_length: Some(0),
            result: JBig2SegmentResult::PatternDictionary(PatternDictionary {
                pattern_width: 1,
                pattern_height: 1,
                patterns: vec![pattern],
            }),
        };
        let payload = make_minimal_halftone_payload();
        let mut stream = make_segment_header(
            2,
            SegmentType::IntermediateHalftoneRegion,
            &[1],
            1,
            u32::try_from(payload.len()).expect("payload length"),
        );
        stream.extend_from_slice(&payload);

        let decoded = decode_segments_with_prior(&stream, Some((1, 1)), &[prior]).expect("decode");

        assert_eq!(decoded.page.get_pixel(0, 0), 0);
        assert_eq!(decoded.segments.len(), 1);
        assert!(matches!(
            decoded.segments.first().map(|segment| &segment.result),
            Some(JBig2SegmentResult::Image(_))
        ));
        let Some(JBig2SegmentResult::Image(image)) =
            decoded.segments.first().map(|segment| &segment.result)
        else {
            return;
        };
        assert_eq!(image.width(), 1);
        assert_eq!(image.height(), 1);
        assert_eq!(image.get_pixel(0, 0), 1);
    }

    #[test]
    fn collective_bitmap_symbol_dictionary_decodes() {
        let dh_table = StandardHuffmanDecoder::new(STANDARD_TABLE_B4).expect("dh");
        let dw_table = StandardHuffmanDecoder::new(STANDARD_TABLE_B2).expect("dw");
        let b1_table = StandardHuffmanDecoder::new(STANDARD_TABLE_B1).expect("b1");
        let mut bits = Vec::new();
        append_huffman_value(&mut bits, &dh_table, HuffmanValue::Value(1));
        append_huffman_value(&mut bits, &dw_table, HuffmanValue::Value(1));
        append_huffman_value(&mut bits, &dw_table, HuffmanValue::OutOfBand);
        append_huffman_value(&mut bits, &b1_table, HuffmanValue::Value(0));
        let mut payload_bits = bits_to_bytes(&bits);

        let mut payload = Vec::new();
        push_u16(&mut payload, 0x0001);
        push_u32(&mut payload, 1);
        push_u32(&mut payload, 1);
        payload.append(&mut payload_bits);
        payload.push(0b1000_0000);
        let mut export_bits = Vec::new();
        append_huffman_value(&mut export_bits, &b1_table, HuffmanValue::Value(0));
        append_huffman_value(&mut export_bits, &b1_table, HuffmanValue::Value(1));
        payload.extend_from_slice(&bits_to_bytes(&export_bits));

        let mut stream = make_segment_header(
            1,
            SegmentType::SymbolDictionary,
            &[],
            1,
            u32::try_from(payload.len()).expect("len"),
        );
        stream.extend_from_slice(&payload);
        let decoded = decode_segments(&stream, None).expect("decode");
        let segment = decoded.segments.first();
        assert!(matches!(
            segment.map(|segment| &segment.result),
            Some(JBig2SegmentResult::SymbolDictionary(_))
        ));
        let Some(JBig2SegmentResult::SymbolDictionary(dict)) =
            segment.map(|segment| &segment.result)
        else {
            return;
        };
        assert_eq!(dict.images.len(), 1);
        let Some(image) = dict.images.first() else {
            return;
        };
        assert_eq!(image.width(), 1);
        assert_eq!(image.height(), 1);
        assert_eq!(image.get_pixel(0, 0), 1);
    }
}
