//! JBIG2 segment-walk orchestration.

use crate::{
    error::Jbig2Error,
    generic_region::GenericRegion,
    halftone_region::decode_halftone_region_segment,
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
            SegmentType::IntermediateGenericRegion | SegmentType::ImmediateGenericRegion => {
                self.compose_generic_region(segment_end)?;
            }
            SegmentType::ImmediateLosslessGenericRegion => {
                self.decode_lossless_generic_region(segment, segment_end)?;
            }
            SegmentType::ImmediateHalftoneRegion | SegmentType::ImmediateLosslessHalftoneRegion => {
                self.decode_immediate_halftone_region(segment, segment_end)?;
            }
            SegmentType::PageInformation => self.initialize_page_from_info()?,
            SegmentType::EndOfPage | SegmentType::EndOfFile => return Ok(false),
            SegmentType::EndOfStripe
            | SegmentType::Profile
            | SegmentType::CodeTable
            | SegmentType::Extension => {}
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

    /// Decode and compose a generic region segment body.
    ///
    /// T.88 section 7.4.6 defines generic region syntax. Immediate and
    /// intermediate generic regions are composed to the current page in this
    /// decoder's supported behavior.
    fn compose_generic_region(&mut self, segment_end: usize) -> Result<(), Jbig2Error> {
        let parsed = GenericRegion::parse(&mut self.stream)?;
        let body = self.generic_region_body(segment_end)?;
        parsed.compose_to(body, &mut self.page)
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

    const BITMAP_HALFTONE_SKIP_GRID_TEMPLATE1_STREAM: &[u8] = &[
        0x00, 0x00, 0x00, 0x00, 0x30, 0x00, 0x01, 0x00, 0x00, 0x00, 0x13, 0x00, 0x00, 0x01, 0x8f,
        0x00, 0x00, 0x01, 0x90, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x01, 0x10, 0x01, 0x01, 0x00, 0x00, 0x02, 0x37, 0x00, 0x10, 0x10, 0x00,
        0x00, 0x00, 0x8b, 0xa7, 0x78, 0x95, 0xf5, 0x15, 0x7a, 0xc7, 0x14, 0x00, 0x7c, 0x3d, 0xdb,
        0xea, 0xaf, 0x4b, 0x76, 0xce, 0x5c, 0x52, 0x49, 0xb6, 0xb2, 0xe3, 0x1b, 0x3d, 0xaf, 0x35,
        0x75, 0xdf, 0xe2, 0xff, 0x49, 0x28, 0xb3, 0x43, 0x47, 0xad, 0x4f, 0x33, 0x6d, 0x56, 0x03,
        0xd2, 0x0e, 0x30, 0x86, 0xd8, 0x02, 0x84, 0xaa, 0xa7, 0xa2, 0xf5, 0xdd, 0x49, 0xd9, 0x55,
        0x1c, 0x80, 0x11, 0xf2, 0xe4, 0xd6, 0xf2, 0x93, 0xbe, 0x35, 0x98, 0xf7, 0x2e, 0xe7, 0x51,
        0xa1, 0x79, 0x82, 0xaa, 0xe6, 0xa4, 0x44, 0x4f, 0x80, 0x6e, 0x84, 0x38, 0xcc, 0x84, 0x45,
        0x86, 0x02, 0x6d, 0x8d, 0x77, 0xb1, 0x9a, 0xbf, 0x11, 0x40, 0x47, 0xf9, 0xa4, 0xfd, 0x06,
        0x3c, 0x70, 0xb5, 0x06, 0xed, 0x65, 0x84, 0x10, 0x2b, 0x8e, 0x59, 0xa3, 0x3e, 0x0b, 0x57,
        0x35, 0x6a, 0x27, 0x1b, 0x9a, 0xf2, 0x2f, 0x50, 0x4a, 0x66, 0xf8, 0x00, 0xf8, 0xab, 0xa9,
        0x30, 0x00, 0xbe, 0x7d, 0x1c, 0x7c, 0x7b, 0x21, 0x69, 0x93, 0xc7, 0xab, 0x63, 0xab, 0x72,
        0xbb, 0x5d, 0xab, 0x3c, 0x7c, 0x6b, 0x2f, 0x90, 0xb4, 0x8e, 0xd9, 0x64, 0x86, 0x3a, 0xce,
        0x7d, 0x36, 0x98, 0x85, 0x7b, 0xe3, 0xd4, 0x7e, 0x01, 0x0b, 0xb0, 0x67, 0x03, 0x48, 0xdc,
        0x9c, 0x83, 0x42, 0x2f, 0x76, 0xeb, 0xce, 0x41, 0xbc, 0x06, 0xaf, 0x3c, 0x1e, 0xa8, 0x1d,
        0x82, 0x20, 0x09, 0x2a, 0x4f, 0xbc, 0xf3, 0xe6, 0x21, 0xb8, 0x75, 0x3e, 0x67, 0x51, 0xe0,
        0xc3, 0x43, 0xdf, 0x6a, 0x12, 0x4e, 0xdf, 0xa0, 0xe2, 0x9b, 0x4d, 0x44, 0x3f, 0x27, 0x52,
        0x31, 0xf5, 0x36, 0x8b, 0x22, 0x51, 0x19, 0xe5, 0xdc, 0x73, 0x6c, 0x31, 0x12, 0x81, 0x26,
        0x99, 0xa9, 0x7a, 0x98, 0x76, 0xb4, 0x00, 0x34, 0xa5, 0x5d, 0xc5, 0x6e, 0x23, 0x5d, 0xef,
        0x40, 0xc6, 0x44, 0x98, 0xe2, 0x4b, 0xcc, 0x12, 0xcd, 0x57, 0x29, 0x16, 0x1f, 0x0b, 0xc2,
        0x48, 0xbf, 0x8e, 0x6a, 0xf8, 0xfe, 0x09, 0xa0, 0xef, 0xdc, 0x5e, 0xbc, 0xeb, 0xa7, 0xce,
        0x5a, 0xbd, 0x7d, 0xa7, 0x27, 0x03, 0xa7, 0xbf, 0x74, 0x06, 0xd4, 0xf9, 0x15, 0xb1, 0x07,
        0x0c, 0xfc, 0xff, 0x41, 0x10, 0x53, 0x20, 0x06, 0xcc, 0x1e, 0xc7, 0x0a, 0x60, 0xb4, 0x58,
        0x72, 0x19, 0x96, 0x02, 0x01, 0x2e, 0x68, 0x6e, 0xb8, 0x91, 0x4b, 0x75, 0x5a, 0xb3, 0x80,
        0x94, 0x05, 0x2b, 0x2d, 0xe6, 0xe3, 0xcf, 0x94, 0x4f, 0x3a, 0x20, 0xb5, 0xe7, 0x49, 0xa5,
        0xf7, 0xbc, 0x20, 0x88, 0x67, 0x9f, 0x02, 0x56, 0xc3, 0x10, 0x34, 0x6b, 0x9b, 0xe9, 0x22,
        0x20, 0x30, 0x6f, 0xd7, 0xbf, 0x25, 0x38, 0x88, 0x47, 0xa0, 0x7b, 0x63, 0xd4, 0x17, 0xa7,
        0xad, 0x79, 0xc6, 0xe0, 0x62, 0x01, 0x07, 0xae, 0xcf, 0x96, 0x01, 0x7e, 0x91, 0x34, 0x99,
        0x5e, 0xfb, 0x0b, 0x30, 0x73, 0xda, 0x44, 0xc7, 0x83, 0xa0, 0x18, 0xd1, 0x8c, 0x05, 0x10,
        0xec, 0x3f, 0x3c, 0x96, 0xec, 0x0e, 0x9f, 0x73, 0xfa, 0xbf, 0x8b, 0x3b, 0x2e, 0xe9, 0x1e,
        0x66, 0xa6, 0xa0, 0x66, 0xff, 0x33, 0xb0, 0xc9, 0x40, 0x36, 0x59, 0xe7, 0x3e, 0x78, 0x03,
        0x19, 0x90, 0x3c, 0xe0, 0x56, 0x8f, 0x26, 0xd9, 0x17, 0xe1, 0xc9, 0xbd, 0xe0, 0x92, 0xb1,
        0xe9, 0x5a, 0x1b, 0xaf, 0x04, 0x6e, 0xdd, 0xf5, 0xcc, 0x36, 0x20, 0x3d, 0xb4, 0x0b, 0x72,
        0x0f, 0x52, 0x4e, 0xdb, 0x76, 0x2a, 0xc9, 0x4d, 0x49, 0x6c, 0xcf, 0xcf, 0x03, 0x75, 0xad,
        0xea, 0xfa, 0xb3, 0xad, 0xda, 0x57, 0x57, 0xb4, 0xdb, 0x19, 0x1e, 0xb0, 0xbf, 0xf6, 0xec,
        0xcb, 0x90, 0xc0, 0x28, 0x7c, 0xf9, 0x9f, 0xe3, 0xad, 0x87, 0xb2, 0xdf, 0x08, 0x8a, 0x41,
        0xef, 0xf3, 0x96, 0x61, 0x42, 0xe1, 0xc0, 0xf9, 0x44, 0x24, 0xa2, 0xea, 0xb8, 0xc5, 0xe0,
        0xbd, 0x17, 0x39, 0xf4, 0x5d, 0x65, 0x54, 0xd6, 0xfb, 0x86, 0x3c, 0x9a, 0x52, 0xfe, 0xd6,
        0x3f, 0x5e, 0xcf, 0x68, 0x90, 0x3a, 0x65, 0x6b, 0x4b, 0x67, 0x05, 0x38, 0x2f, 0x48, 0x61,
        0x3f, 0x27, 0x45, 0x14, 0x55, 0x8b, 0xff, 0xac, 0x00, 0x00, 0x00, 0x02, 0x17, 0x20, 0x01,
        0x01, 0x00, 0x00, 0x00, 0xf8, 0x00, 0x00, 0x01, 0x8f, 0x00, 0x00, 0x01, 0x90, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0a, 0x00, 0x00, 0x00, 0x2b, 0x00, 0x00, 0x00,
        0x29, 0xff, 0xff, 0x8e, 0x18, 0x00, 0x00, 0x28, 0xb8, 0x0c, 0x00, 0x04, 0x00, 0xaa, 0xdf,
        0xac, 0x36, 0x07, 0x21, 0x00, 0x2e, 0x45, 0x0b, 0x7e, 0xda, 0x5d, 0x68, 0xdf, 0x2c, 0xe4,
        0xa8, 0x0f, 0x54, 0x63, 0x54, 0xcd, 0x10, 0xc9, 0x31, 0x73, 0x2b, 0xcd, 0x82, 0x4c, 0x25,
        0xcd, 0x63, 0xf8, 0xa9, 0x9d, 0xac, 0xb7, 0xa1, 0xf0, 0x99, 0xa1, 0x32, 0x64, 0xd9, 0xeb,
        0xac, 0x02, 0x5c, 0x2e, 0x5a, 0x60, 0x62, 0x05, 0xa3, 0x96, 0x1c, 0xa9, 0x19, 0x3b, 0x84,
        0xb6, 0xc1, 0x10, 0x73, 0x72, 0x30, 0x31, 0x4d, 0x4d, 0xf6, 0x01, 0x9b, 0xf4, 0x47, 0x08,
        0xc7, 0x6a, 0xa9, 0x36, 0xcf, 0x20, 0x0a, 0x7d, 0x71, 0x16, 0x12, 0x4b, 0xda, 0x91, 0xea,
        0x64, 0x58, 0x35, 0xd5, 0x0c, 0x74, 0x3d, 0xae, 0xf4, 0x27, 0x60, 0xaf, 0x52, 0x96, 0xad,
        0x6a, 0x86, 0x64, 0x04, 0xb0, 0x23, 0xcc, 0x35, 0x12, 0x72, 0x59, 0x69, 0xb6, 0x76, 0x22,
        0xd5, 0x2c, 0xc1, 0xd6, 0xd9, 0x34, 0x4e, 0x36, 0xa2, 0xbf, 0x5d, 0x5f, 0x74, 0x13, 0xb0,
        0xa0, 0x94, 0x60, 0x50, 0xbe, 0x10, 0x81, 0x73, 0x4a, 0x12, 0x91, 0x4d, 0x7b, 0xde, 0x71,
        0xf5, 0x57, 0x26, 0x29, 0x59, 0xf2, 0x47, 0x25, 0xb0, 0x3f, 0xc1, 0x92, 0x19, 0x17, 0x19,
        0x50, 0xb4, 0x9f, 0x9e, 0x71, 0x09, 0xdd, 0x3d, 0x90, 0x4b, 0x1a, 0x8b, 0xab, 0xbf, 0x1c,
        0xe7, 0x71, 0xcf, 0x0e, 0xe2, 0x8a, 0xaf, 0xee, 0xa7, 0x9a, 0x5d, 0x66, 0x5c, 0xf5, 0x23,
        0xc1, 0x08, 0x76, 0x0f, 0xcc, 0x9b, 0x8a, 0xd1, 0x89, 0xbe, 0xb7, 0xff, 0xac,
    ];

    #[test]
    fn halftone_skip_grid_regression_decodes_to_expected_page_size() {
        let decoded = decode_segments(BITMAP_HALFTONE_SKIP_GRID_TEMPLATE1_STREAM, Some((399, 400)))
            .expect("decode");
        assert!(decoded.segments.len() >= 2);
        assert_eq!(decoded.page.width(), 399);
        assert_eq!(decoded.page.height(), 400);
    }
}
