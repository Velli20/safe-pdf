//! JBIG2 Huffman text-region decoder.

use crate::{
    arith_decoder::JBig2ArithDecoder,
    error::Jbig2Error,
    huffman::{
        CustomHuffmanDecoder, CustomHuffmanTableCursor, HuffmanDecoder, HuffmanTableSelection,
        HuffmanValue, SymbolIdHuffmanTable, decode_symbol_id, decode_symbol_id_huffman_table,
    },
    image::JBig2Image,
    segment_context::SegmentDecodeContext,
    text_region::{
        flags::TextRegionFlagBits,
        huffman_flags::TextRegionHuffmanFlags,
        parser::ParsedTextRegion,
        refinement::{DecodedTextRegionInstance, TextRegionDecodeContext},
        state::TextRegionDecodeState,
        strip_decode_driver::{
            TextRegionRefinedInstanceDecodeDriver, TextRegionStripDecodeDriver, decode_text_region,
        },
    },
    util::{INTEGER_CONVERSION_OVERFLOW, ceil_log2},
};
use pdf_utils::BitReader;

const TEXT_REGION_BODY: &str = "text region body";
const TEXT_REGION_STRIP_INDEX: &str = "text region strip index";
const TEXT_REGION_REFINEMENT_TABLES: &str = "text-region refinement Huffman tables";
const TEXT_REGION_REFINEMENT_FLAG: &str = "text region refinement flag";
const TEXT_REGION_REFINEMENT_WIDTH: &str = "text region refinement width";
const TEXT_REGION_REFINEMENT_HEIGHT: &str = "text region refinement height";

/// Parsed and resolved state needed by the Huffman text-region procedure.
///
/// ITU-T T.88 | ISO/IEC 14492 section 6.4.5 uses the parsed segment header,
/// referred symbol dictionaries, three standard Huffman tables, and a symbol
/// ID table to place each symbol instance into the region bitmap.
struct HuffmanTextRegionDecodeContext<'a> {
    shared: TextRegionDecodeContext<'a>,
    fs_table: HuffmanDecoder,
    ds_table: HuffmanDecoder,
    dt_table: HuffmanDecoder,
    symbol_id_table: SymbolIdHuffmanTable,
    refinement_tables: Option<TextRegionRefinementTables>,
    body: &'a [u8],
}

/// Standard Huffman tables used by refinement-coded text regions.
#[derive(Debug, Clone)]
struct TextRegionRefinementTables {
    rsize_table: HuffmanDecoder,
    rdw_table: HuffmanDecoder,
    rdh_table: HuffmanDecoder,
    rdx_table: HuffmanDecoder,
    rdy_table: HuffmanDecoder,
}

/// Decode a Huffman-coded JBIG2 text-region body.
///
/// ITU-T T.88 | ISO/IEC 14492 section 7.4.3 selects this path when
/// `SBHUFF = 1`; section 6.4.5 defines the symbol placement loop.
pub(crate) fn decode_huffman_text_region(
    context: &SegmentDecodeContext<'_, '_, '_, '_, '_>,
    parsed: ParsedTextRegion<'_>,
) -> Result<JBig2Image, Jbig2Error> {
    HuffmanTextRegionDecoder::new(context, parsed)?.decode()
}

impl<'a> HuffmanTextRegionDecodeContext<'a> {
    /// Resolve dictionaries, Huffman tables, and the symbol-ID table.
    ///
    /// ITU-T T.88 | ISO/IEC 14492 section 7.4.3.1.2 defines the table
    /// selectors. Section 6.4.10 defines symbol-ID decoding.
    fn new(
        context: &SegmentDecodeContext<'_, '_, '_, '_, '_>,
        parsed: ParsedTextRegion<'a>,
    ) -> Result<Self, Jbig2Error> {
        parsed.validate_supported_huffman_text_region()?;
        let shared = TextRegionDecodeContext::new(context, parsed)?;
        let huffman_flags = parsed
            .huffman_flags
            .ok_or(Jbig2Error::InvalidState("text region Huffman flags"))?;
        let custom_tables = referred_huffman_tables(context)?;
        let mut custom_tables = CustomHuffmanTableCursor::new(custom_tables);
        let fs_table = custom_tables.text_region_table(HuffmanTableSelection::TextRegionFs(
            huffman_flags.fs_selector,
        ))?;
        let ds_table = custom_tables.text_region_table(HuffmanTableSelection::TextRegionDs(
            huffman_flags.ds_selector,
        ))?;
        let dt_table = custom_tables.text_region_table(HuffmanTableSelection::TextRegionDt(
            huffman_flags.dt_selector,
        ))?;
        let mut body_reader = BitReader::new(parsed.body);
        let symbol_id_table =
            decode_symbol_id_huffman_table(&mut body_reader, shared.symbols_len())?;
        body_reader.align_to_byte_boundary();
        let body = body_reader
            .remaining_from_byte()
            .ok_or(Jbig2Error::Truncated(TEXT_REGION_BODY))?;
        let refinement_tables = if parsed.flags.contains(TextRegionFlagBits::SBREFINE) {
            Some(TextRegionRefinementTables::new(
                &mut custom_tables,
                huffman_flags,
            )?)
        } else {
            None
        };

        Ok(Self {
            shared,
            fs_table,
            ds_table,
            dt_table,
            symbol_id_table,
            refinement_tables,
            body,
        })
    }
}

fn referred_huffman_tables(
    context: &SegmentDecodeContext<'_, '_, '_, '_, '_>,
) -> Result<Vec<CustomHuffmanDecoder>, Jbig2Error> {
    let mut tables = Vec::new();
    for index in 0usize.. {
        match context.referred_huffman_table(index) {
            Ok(table) => tables.push(table.clone()),
            Err(Jbig2Error::MissingSegment) => break,
            Err(err) => return Err(err),
        }
    }
    Ok(tables)
}

impl TextRegionRefinementTables {
    /// Resolve the Huffman tables used by refinement-coded instances.
    fn new(
        custom_tables: &mut CustomHuffmanTableCursor,
        huffman_flags: TextRegionHuffmanFlags,
    ) -> Result<Self, Jbig2Error> {
        let rdw_table = custom_tables.text_region_refinement_table(huffman_flags.rdw_selector)?;
        let rdh_table = custom_tables.text_region_refinement_table(huffman_flags.rdh_selector)?;
        let rdx_table = custom_tables.text_region_refinement_table(huffman_flags.rdx_selector)?;
        let rdy_table = custom_tables.text_region_refinement_table(huffman_flags.rdy_selector)?;
        let rsize_table = custom_tables.text_region_rsize_table(huffman_flags.rsize_custom)?;

        Ok(Self {
            rsize_table,
            rdw_table,
            rdh_table,
            rdx_table,
            rdy_table,
        })
    }
}

/// Decoder for the Huffman text-region procedure.
///
/// ITU-T T.88 | ISO/IEC 14492 section 6.4.5 describes the strip loop and
/// symbol-instance loop modeled by this type.
struct HuffmanTextRegionDecoder<'a> {
    context: HuffmanTextRegionDecodeContext<'a>,
    body_reader: BitReader<'a>,
    state: TextRegionDecodeState,
}

impl<'a> HuffmanTextRegionDecoder<'a> {
    /// Create a Huffman text-region decoder from the parsed segment.
    ///
    /// ITU-T T.88 | ISO/IEC 14492 section 6.4.5 step 2 decodes the initial
    /// strip delta before entering the main strip loop.
    fn new(
        context: &SegmentDecodeContext<'_, '_, '_, '_, '_>,
        parsed: ParsedTextRegion<'a>,
    ) -> Result<Self, Jbig2Error> {
        let context = HuffmanTextRegionDecodeContext::new(context, parsed)?;
        let mut body_reader = BitReader::new(context.body);
        let initial_stript = context.dt_table.decode_value(&mut body_reader)?;
        let state = TextRegionDecodeState::from_initial_delta(
            initial_stript,
            context.shared.parsed().flags.sbstrips(),
        )?;

        Ok(Self {
            context,
            body_reader,
            state,
        })
    }

    /// Decode all symbol instances into the text-region bitmap.
    ///
    /// This implements ITU-T T.88 | ISO/IEC 14492 section 6.4.5 step 3 until
    /// `NINSTANCES` reaches `SBNUMINSTANCES`.
    fn decode(mut self) -> Result<JBig2Image, Jbig2Error> {
        decode_text_region(&mut self)?;

        Ok(self.context.shared.into_region())
    }
}

impl TextRegionRefinedInstanceDecodeDriver for HuffmanTextRegionDecoder<'_> {
    fn with_context_and_state<T>(
        &mut self,
        f: impl FnOnce(&mut TextRegionDecodeContext<'_>, &mut TextRegionDecodeState) -> T,
    ) -> T {
        f(&mut self.context.shared, &mut self.state)
    }

    fn decode_refined_instance_image(
        &mut self,
        instance: DecodedTextRegionInstance,
    ) -> Result<JBig2Image, Jbig2Error> {
        let tables = self
            .context
            .refinement_tables
            .as_ref()
            .ok_or(Jbig2Error::InvalidState(TEXT_REGION_REFINEMENT_TABLES))?;
        let delta_width = tables.rdw_table.decode_value(&mut self.body_reader)?;
        let delta_height = tables.rdh_table.decode_value(&mut self.body_reader)?;
        let delta_x = tables.rdx_table.decode_value(&mut self.body_reader)?;
        let delta_y = tables.rdy_table.decode_value(&mut self.body_reader)?;
        let refinement_size = tables.rsize_table.decode_value(&mut self.body_reader)?;
        self.body_reader.align_to_byte_boundary();
        let refinement_size = usize::try_from(refinement_size)
            .map_err(|_| Jbig2Error::Overflow(INTEGER_CONVERSION_OVERFLOW))?;
        let refinement_body = self
            .body_reader
            .take_from_byte_len(refinement_size)
            .ok_or(Jbig2Error::Truncated(TEXT_REGION_REFINEMENT_TABLES))?;
        let image = {
            let mut refinement_reader = BitReader::new(refinement_body);
            let mut arith_decoder = JBig2ArithDecoder::new(&mut refinement_reader);
            self.context.shared.decode_refined_image(
                instance.symbol_id,
                delta_width,
                delta_height,
                delta_x,
                delta_y,
                TEXT_REGION_REFINEMENT_WIDTH,
                TEXT_REGION_REFINEMENT_HEIGHT,
                |size_delta, delta| {
                    size_delta
                        .checked_shr(2)
                        .and_then(|value| value.checked_add(delta))
                        .ok_or(Jbig2Error::Overflow(INTEGER_CONVERSION_OVERFLOW))
                },
                &mut arith_decoder,
            )?
        };
        self.body_reader.align_to_byte_boundary();
        Ok(image)
    }
}

impl TextRegionStripDecodeDriver for HuffmanTextRegionDecoder<'_> {
    fn context(&self) -> &TextRegionDecodeContext<'_> {
        &self.context.shared
    }

    fn state(&self) -> &TextRegionDecodeState {
        &self.state
    }

    fn state_mut(&mut self) -> &mut TextRegionDecodeState {
        &mut self.state
    }

    /// Decode the next strip `DT` header.
    ///
    /// ITU-T T.88 | ISO/IEC 14492 sections 6.4.5 step 3(b) and 6.4.6 define
    /// strip delta decoding and `STRIPT` advancement.
    fn decode_next_strip_header_delta(&mut self) -> Result<i32, Jbig2Error> {
        self.context.dt_table.decode_value(&mut self.body_reader)
    }

    /// Decode the next symbol-instance position within the current strip.
    ///
    /// ITU-T T.88 | ISO/IEC 14492 section 6.4.5 step 3(c), section 6.4.7 for
    /// first-symbol `S`, section 6.4.8 for subsequent `S`, section 6.4.9 for
    /// `T`, and section 6.4.10 for symbol ID.
    fn decode_first_symbol_delta(&mut self) -> Result<i32, Jbig2Error> {
        self.context.fs_table.decode_value(&mut self.body_reader)
    }

    fn decode_delta_s_or_end(&mut self) -> Result<Option<i32>, Jbig2Error> {
        match self.context.ds_table.decode(&mut self.body_reader)? {
            HuffmanValue::OutOfBand => Ok(None),
            HuffmanValue::Value(delta_s) => Ok(Some(delta_s)),
        }
    }

    fn decode_current_t(&mut self) -> Result<i64, Jbig2Error> {
        let sbstrips = self.context.shared.parsed().flags.sbstrips();
        let current_t = if sbstrips == 1 {
            0
        } else {
            let bits = ceil_log2(usize::from(sbstrips))?;
            i64::from(
                self.body_reader
                    .read_bits(bits)
                    .ok_or(Jbig2Error::Truncated(TEXT_REGION_STRIP_INDEX))?,
            )
        };
        Ok(current_t)
    }

    fn decode_symbol_id(&mut self) -> Result<usize, Jbig2Error> {
        decode_symbol_id(&mut self.body_reader, &self.context.symbol_id_table)
    }

    fn decode_refinement_flag(&mut self) -> Result<bool, Jbig2Error> {
        if self
            .context
            .shared
            .parsed()
            .flags
            .contains(TextRegionFlagBits::SBREFINE)
        {
            Ok(self
                .body_reader
                .next_bit()
                .ok_or(Jbig2Error::Truncated(TEXT_REGION_REFINEMENT_FLAG))?)
        } else {
            Ok(false)
        }
    }
}

#[cfg(test)]
mod tests {
    use pdf_utils::BitReader;

    use crate::{
        error::Jbig2Error,
        huffman::{
            CustomHuffmanDecoder, HuffmanTableSelection, HuffmanValue, StandardHuffmanDecoder,
            test_support::{bits_for_value, bits_to_bytes, push_bits},
            text_region_refinement_standard_decoder,
        },
        image::JBig2Image,
        segment::{JBig2SegmentResult, ParsedSegment},
        segment_context::SegmentDecodeContext,
        stream::decode_segments,
        symbol_dictionary::SymbolDictionary,
        text_region::{
            flags::TextRegionFlagBits,
            geometry::{TextRegionGeometry, TextRegionRefCorner},
            huffman::decode_huffman_text_region,
            parser::ParsedTextRegion,
        },
    };

    #[derive(Clone)]
    struct BitWriter {
        bits: Vec<bool>,
    }

    impl BitWriter {
        fn new() -> Self {
            Self { bits: Vec::new() }
        }

        fn push_bits(&mut self, value: u32, width: u8) {
            for shift in (0..u32::from(width)).rev() {
                self.bits.push(((value >> shift) & 1) != 0);
            }
        }

        fn align_to_byte(&mut self) {
            let rem = self.bits.len() % 8;
            if rem != 0 {
                for _ in rem..8 {
                    self.bits.push(false);
                }
            }
        }

        fn push_bytes(&mut self, bytes: &[u8]) {
            self.align_to_byte();
            for byte in bytes {
                self.push_bits(u32::from(*byte), 8);
            }
        }

        fn into_bytes(self) -> Vec<u8> {
            let mut bytes = Vec::new();
            let mut current = 0u8;
            for (index, bit) in self.bits.into_iter().enumerate() {
                current = (current << 1) | u8::from(bit);
                if index % 8 == 7 {
                    bytes.push(current);
                    current = 0;
                }
            }
            bytes
        }
    }

    fn push_huffman_result(
        writer: &mut BitWriter,
        table: &StandardHuffmanDecoder,
        result: HuffmanValue,
    ) {
        let (code, codelen, extra, extra_len) =
            bits_for_value(table, result).expect("encodable value");
        writer.push_bits(code, codelen);
        writer.push_bits(extra, extra_len);
    }

    fn push_first_encodable_value(
        writer: &mut BitWriter,
        table: &StandardHuffmanDecoder,
        candidates: &[i32],
    ) -> Result<i32, Jbig2Error> {
        for candidate in candidates {
            let value = HuffmanValue::Value(*candidate);
            if let Some((code, codelen, extra, extra_len)) = bits_for_value(table, value) {
                writer.push_bits(code, codelen);
                writer.push_bits(extra, extra_len);
                return Ok(*candidate);
            }
        }
        Err(Jbig2Error::InvalidState("encodable Huffman test value"))
    }

    fn push_single_symbol_id_table(writer: &mut BitWriter) {
        for index in 0..35usize {
            let value = if index == 1 { 1 } else { 0 };
            writer.push_bits(value, 4);
        }
        writer.push_bits(0, 1);
        writer.align_to_byte();
    }

    fn push_custom_table_header(
        bytes: &mut Vec<u8>,
        flags: u8,
        lowest_value: i32,
        highest_value: i32,
    ) {
        bytes.push(flags);
        bytes.extend_from_slice(&lowest_value.to_be_bytes());
        bytes.extend_from_slice(&highest_value.to_be_bytes());
    }

    fn single_value_custom_table(value: i32) -> CustomHuffmanDecoder {
        let highest_value = value.checked_add(1).expect("highest value");
        let mut data = Vec::new();
        push_custom_table_header(&mut data, 0b0000_0010, value, highest_value);
        let mut bits = Vec::new();
        push_bits(&mut bits, 1, 2);
        push_bits(&mut bits, 0, 1);
        push_bits(&mut bits, 2, 2);
        push_bits(&mut bits, 2, 2);
        data.extend_from_slice(&bits_to_bytes(&bits));

        CustomHuffmanDecoder::parse(&data).expect("custom table")
    }

    fn solid_symbol(width: u16, height: u16) -> JBig2Image {
        let mut image = JBig2Image::new(width, height);
        for y in 0..height {
            for x in 0..width {
                image.set_pixel(x, y, 1);
            }
        }
        image
    }

    fn dictionary_segment(number: u32, images: Vec<JBig2Image>) -> ParsedSegment {
        ParsedSegment {
            number,
            flags: 0,
            referred_to_segment_numbers: Vec::new(),
            page_association: 0,
            data_length: None,
            result: JBig2SegmentResult::SymbolDictionary(SymbolDictionary { images }),
        }
    }

    fn text_region_segment(number: u32, referred_number: u32) -> ParsedSegment {
        ParsedSegment {
            number,
            flags: 0,
            referred_to_segment_numbers: vec![referred_number],
            page_association: 0,
            data_length: None,
            result: JBig2SegmentResult::None,
        }
    }

    fn huffman_table_segment(number: u32, table: CustomHuffmanDecoder) -> ParsedSegment {
        ParsedSegment {
            number,
            flags: 0,
            referred_to_segment_numbers: Vec::new(),
            page_association: 0,
            data_length: None,
            result: JBig2SegmentResult::HuffmanTable(table),
        }
    }

    fn text_region_segment_with_references(
        number: u32,
        referred_numbers: Vec<u32>,
    ) -> ParsedSegment {
        ParsedSegment {
            number,
            flags: 0,
            referred_to_segment_numbers: referred_numbers,
            page_association: 0,
            data_length: None,
            result: JBig2SegmentResult::None,
        }
    }

    fn build_text_region_data(
        width: u16,
        height: u16,
        flags: u16,
        huffman_flags: u16,
        symbol_instances: u32,
        body: Vec<u8>,
    ) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&u32::from(width).to_be_bytes());
        data.extend_from_slice(&u32::from(height).to_be_bytes());
        data.extend_from_slice(&0u32.to_be_bytes());
        data.extend_from_slice(&0u32.to_be_bytes());
        data.push(0);
        data.extend_from_slice(&flags.to_be_bytes());
        data.extend_from_slice(&huffman_flags.to_be_bytes());
        data.extend_from_slice(&symbol_instances.to_be_bytes());
        data.extend_from_slice(&body);
        data
    }

    fn text_region_tables() -> (
        StandardHuffmanDecoder,
        StandardHuffmanDecoder,
        StandardHuffmanDecoder,
    ) {
        (
            HuffmanTableSelection::TextRegionFs(0)
                .standard_decoder()
                .expect("fs"),
            HuffmanTableSelection::TextRegionDs(0)
                .standard_decoder()
                .expect("ds"),
            HuffmanTableSelection::TextRegionDt(2)
                .standard_decoder()
                .expect("dt"),
        )
    }

    fn top_left_corner_bits() -> u16 {
        1 << 4
    }

    fn dt_selector_2_bits() -> u16 {
        2 << 4
    }

    fn lit_pixels(image: &JBig2Image) -> Vec<(usize, usize)> {
        let mut pixels = Vec::new();
        for y in 0..usize::from(image.height()) {
            for x in 0..usize::from(image.width()) {
                if image.get_pixel(
                    u16::try_from(x).expect("x fits in u16"),
                    u16::try_from(y).expect("y fits in u16"),
                ) != 0
                {
                    pixels.push((x, y));
                }
            }
        }
        pixels
    }

    pub(crate) fn decode_huffman_text_region_segment(
        segment: &ParsedSegment,
        data: &[u8],
        prior_segments: &[ParsedSegment],
    ) -> Result<JBig2Image, Jbig2Error> {
        let parsed = ParsedTextRegion::try_from(data)?;
        let mut stream = BitReader::new(&[]);
        let context = SegmentDecodeContext::new(segment, &mut stream, 0, &[], prior_segments);
        decode_huffman_text_region(&context, parsed)
    }

    fn textrefine_jbig2_stream() -> &'static [u8] {
        const BYTES: &[u8] = &[
            0x00, 0x00, 0x00, 0x00, 0x30, 0x00, 0x01, 0x00, 0x00, 0x00, 0x13, 0x00, 0x00, 0x01,
            0x8f, 0x00, 0x00, 0x01, 0x90, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x01, 0x00, 0x00, 0x01, 0x52, 0x00,
            0x01, 0x00, 0x00, 0x00, 0x07, 0x00, 0x00, 0x00, 0x07, 0xf2, 0x5e, 0x4d, 0xfc, 0x2c,
            0xf2, 0x40, 0xca, 0x81, 0xc2, 0x0c, 0x20, 0xe1, 0x06, 0x10, 0x74, 0xd3, 0xa6, 0x9f,
            0xe9, 0xa7, 0xfa, 0x69, 0xff, 0xff, 0xff, 0xff, 0x69, 0xaf, 0xda, 0x6b, 0xf6, 0x9a,
            0xb4, 0xd4, 0x30, 0x83, 0x0a, 0x18, 0x41, 0x85, 0x11, 0xe0, 0xed, 0xe7, 0x7c, 0x4f,
            0xf0, 0x80, 0x21, 0x40, 0x5e, 0xd6, 0x15, 0x85, 0x21, 0x8d, 0xa8, 0x41, 0x86, 0x14,
            0x20, 0xc3, 0x0a, 0x98, 0xd3, 0xf4, 0xfd, 0x3f, 0xff, 0xfe, 0xd7, 0xb5, 0xed, 0x5a,
            0x86, 0x14, 0x30, 0xa3, 0xff, 0xf0, 0xf1, 0x1e, 0x77, 0xc4, 0x7f, 0x08, 0x00, 0xf9,
            0x35, 0x3c, 0x3c, 0x3d, 0xef, 0xef, 0xef, 0xff, 0xf7, 0xff, 0xff, 0x7f, 0x7f, 0x7f,
            0xef, 0xef, 0xfd, 0xfd, 0xfd, 0xfd, 0xfd, 0xfd, 0xdf, 0x77, 0xdd, 0xf7, 0x7d, 0xfd,
            0xdf, 0x77, 0x80, 0xf8, 0x00, 0x00, 0x03, 0x97, 0xc0, 0x00, 0x00, 0x05, 0xbf, 0xd6,
            0x40, 0xff, 0x29, 0x07, 0x08, 0x86, 0x37, 0x50, 0x41, 0xd0, 0x74, 0x1e, 0x1d, 0x3c,
            0x3a, 0x0f, 0x7b, 0xde, 0x1e, 0xf7, 0x7d, 0xdf, 0x0e, 0xdd, 0xb8, 0x78, 0x6e, 0x44,
            0x83, 0x4a, 0x1b, 0x87, 0xb7, 0x0f, 0x6e, 0xdd, 0xe1, 0xbd, 0xdb, 0xf6, 0xef, 0x6f,
            0xde, 0xdf, 0xb7, 0xbb, 0x77, 0xb7, 0xf7, 0x7f, 0x6f, 0xdb, 0xbd, 0xf7, 0x7d, 0xee,
            0xfb, 0xbe, 0xf7, 0x7d, 0xdf, 0x76, 0xef, 0xbb, 0x7b, 0xb7, 0x7d, 0xdf, 0x77, 0xb7,
            0xed, 0xfb, 0x77, 0xb7, 0x6f, 0xdb, 0xf6, 0xf7, 0x7d, 0xdf, 0x77, 0xbf, 0xbe, 0xef,
            0x7f, 0xf7, 0xef, 0xff, 0xdd, 0xff, 0xff, 0x77, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff, 0xff, 0xeb, 0xeb, 0xaf, 0xaa, 0xfa, 0xea, 0xba, 0xaf, 0xad,
            0x2f, 0x4b, 0xd2, 0xf0, 0x95, 0x69, 0x7a, 0x5a, 0xae, 0xab, 0xaa, 0xea, 0xba, 0xd5,
            0x05, 0xe9, 0x56, 0x97, 0xa5, 0x05, 0x85, 0x82, 0x50, 0xb0, 0x94, 0x2c, 0x25, 0x09,
            0x42, 0x50, 0xb5, 0xa5, 0x0b, 0x4a, 0xb0, 0x94, 0x25, 0x08, 0x28, 0x41, 0x42, 0x0a,
            0x10, 0x52, 0x0c, 0x14, 0xa1, 0x05, 0x04, 0x0a, 0x50, 0x07, 0x10, 0x82, 0x84, 0x15,
            0x05, 0x41, 0x6a, 0x96, 0x15, 0x05, 0xad, 0x61, 0x61, 0x61, 0x61, 0x58, 0x58, 0x56,
            0x16, 0x15, 0x90, 0x51, 0xb4, 0x8c, 0x14, 0x30, 0xa1, 0x91, 0xec, 0x5c, 0x70, 0x01,
            0xc0, 0x00, 0x00, 0x00, 0x02, 0x00, 0x21, 0x01, 0x01, 0x00, 0x00, 0x00, 0x9d, 0x00,
            0x03, 0xff, 0xff, 0xff, 0xff, 0x00, 0x00, 0x00, 0x0b, 0x00, 0x00, 0x00, 0x04, 0xf6,
            0x1e, 0xc0, 0x2a, 0x43, 0xc0, 0xa9, 0x09, 0x94, 0xda, 0xd8, 0x47, 0x09, 0x93, 0x02,
            0xd8, 0x03, 0x5b, 0x11, 0xba, 0xcd, 0x70, 0xc1, 0x8d, 0x27, 0x11, 0x7d, 0x49, 0xd7,
            0x52, 0x54, 0x14, 0x04, 0xd5, 0x5f, 0x53, 0x85, 0x62, 0xd2, 0x74, 0xd0, 0x9c, 0x37,
            0x11, 0x6c, 0x24, 0x20, 0x0b, 0xcd, 0x6b, 0xff, 0xac, 0xf3, 0xa2, 0x00, 0x0e, 0xd2,
            0x10, 0x60, 0x8c, 0x40, 0xf0, 0xa9, 0x4c, 0x81, 0x4d, 0xbe, 0x45, 0x98, 0x3c, 0x33,
            0xc5, 0xc0, 0xf2, 0x15, 0xcf, 0xf9, 0xed, 0xbb, 0x66, 0xf5, 0x2d, 0xf3, 0xc8, 0x11,
            0xe0, 0x9b, 0x26, 0x25, 0x2e, 0xfc, 0xaa, 0x89, 0x84, 0xc2, 0x7b, 0xbd, 0xe2, 0xad,
            0xc5, 0x98, 0xfe, 0xe5, 0x7f, 0x8d, 0xc6, 0x67, 0xc1, 0xe9, 0xcd, 0x6a, 0xf7, 0xde,
            0xeb, 0x9c, 0x26, 0xfb, 0x48, 0xd8, 0xf4, 0x60, 0xac, 0x30, 0xef, 0xff, 0xac, 0xff,
            0xce, 0x7c, 0x00, 0x00, 0x00, 0x32, 0x30, 0x03, 0x27, 0xeb, 0xef, 0xe6, 0x24, 0x7f,
            0x02, 0xc0, 0x00, 0x00, 0x00, 0x03, 0x06, 0x20, 0x02, 0x01, 0x00, 0x00, 0x00, 0x77,
            0x00, 0x00, 0x01, 0x8f, 0x00, 0x00, 0x01, 0x90, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x0d, 0x17, 0x00, 0x00, 0xff, 0xff, 0xff, 0xff, 0x00, 0x00, 0x00,
            0x04, 0x00, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x01, 0x02, 0x50, 0x9f, 0x21, 0x68, 0x47, 0xcb, 0xaf, 0xb8, 0x44,
            0xe0, 0xab, 0x4b, 0x8f, 0x97, 0x4d, 0xb6, 0x3d, 0xe0, 0xe8, 0x6f, 0xff, 0x7f, 0xff,
            0x74, 0xda, 0xb0, 0x28, 0x47, 0x39, 0xff, 0x7f, 0x56, 0xa2, 0xea, 0x49, 0xa4, 0x74,
            0x77, 0x9a, 0xa3, 0x15, 0xb5, 0x24, 0x40, 0x45, 0x3e, 0xc0, 0xa6, 0xd1, 0x53, 0x23,
            0x15, 0x16, 0xc6, 0x07, 0xf9, 0x8f, 0xbc, 0xe7, 0x13, 0xaf, 0x17, 0x3f, 0xff, 0xac,
            0x73, 0xb5, 0x93, 0xf8, 0x10, 0x54, 0x20,
        ];

        BYTES
    }

    fn decode_sample_segment_dictionaries() -> (Vec<ParsedSegment>, ParsedSegment, Vec<u8>) {
        let data = textrefine_jbig2_stream();
        let mut reader = BitReader::new(data);
        let mut decoded_segments = Vec::new();

        let page_segment = ParsedSegment::try_from(&mut reader).expect("page segment");
        let page_end = reader.byte_pos() + page_segment.data_length.expect("page length");
        reader.set_byte_pos_preserving_offset(page_end);
        decoded_segments.push(page_segment.clone());

        let mut first_dict_segment =
            ParsedSegment::try_from(&mut reader).expect("first dictionary segment");
        let first_dict_end = reader.byte_pos()
            + first_dict_segment
                .data_length
                .expect("first dictionary length");
        {
            let mut context = SegmentDecodeContext::new(
                &first_dict_segment,
                &mut reader,
                first_dict_end,
                &decoded_segments,
                &[],
            );
            first_dict_segment.result = JBig2SegmentResult::SymbolDictionary(
                SymbolDictionary::from_reader(&mut context).expect("first dictionary"),
            );
        }
        reader.set_byte_pos_preserving_offset(first_dict_end);
        decoded_segments.push(first_dict_segment);

        let mut second_dict_segment =
            ParsedSegment::try_from(&mut reader).expect("second dictionary segment");
        let second_dict_end = reader.byte_pos()
            + second_dict_segment
                .data_length
                .expect("second dictionary length");
        {
            let mut context = SegmentDecodeContext::new(
                &second_dict_segment,
                &mut reader,
                second_dict_end,
                &decoded_segments,
                &[],
            );
            second_dict_segment.result = JBig2SegmentResult::SymbolDictionary(
                SymbolDictionary::from_reader(&mut context).expect("second dictionary"),
            );
        }
        reader.set_byte_pos_preserving_offset(second_dict_end);
        decoded_segments.push(second_dict_segment);

        let text_segment = ParsedSegment::try_from(&mut reader).expect("text segment");
        let text_len = text_segment.data_length.expect("text length");
        let text_body = data[reader.byte_pos()..reader.byte_pos() + text_len].to_vec();

        (decoded_segments, text_segment, text_body)
    }

    #[test]
    fn single_symbol_single_strip_places_symbol_at_expected_coordinates() {
        let (fs_table, _, dt_table) = text_region_tables();
        let mut writer = BitWriter::new();
        push_single_symbol_id_table(&mut writer);
        push_huffman_result(&mut writer, &dt_table, HuffmanValue::Value(1));
        push_huffman_result(&mut writer, &dt_table, HuffmanValue::Value(4));
        push_huffman_result(&mut writer, &fs_table, HuffmanValue::Value(2));
        writer.push_bits(0, 1);
        writer.align_to_byte();

        let flags = TextRegionFlagBits::SBHUFF.bits();
        let data =
            build_text_region_data(8, 8, flags, dt_selector_2_bits(), 1, writer.into_bytes());
        let dict = dictionary_segment(1, vec![solid_symbol(2, 2)]);
        let segment = text_region_segment(2, 1);

        let image =
            decode_huffman_text_region_segment(&segment, &data, &[dict]).expect("decode region");
        assert_eq!(lit_pixels(&image), vec![(2, 2), (3, 2), (2, 3), (3, 3)]);
    }

    #[test]
    fn ds_oob_terminates_current_strip_before_next_strip_header() {
        let (fs_table, ds_table, dt_table) = text_region_tables();
        let mut writer = BitWriter::new();
        push_single_symbol_id_table(&mut writer);
        push_huffman_result(&mut writer, &dt_table, HuffmanValue::Value(1));
        push_huffman_result(&mut writer, &dt_table, HuffmanValue::Value(1));
        push_huffman_result(&mut writer, &fs_table, HuffmanValue::Value(1));
        writer.push_bits(0, 1);
        push_huffman_result(&mut writer, &ds_table, HuffmanValue::OutOfBand);
        push_huffman_result(&mut writer, &dt_table, HuffmanValue::Value(1));
        push_huffman_result(&mut writer, &fs_table, HuffmanValue::Value(1));
        writer.push_bits(0, 1);
        writer.align_to_byte();

        let flags = TextRegionFlagBits::SBHUFF.bits() | top_left_corner_bits();
        let data =
            build_text_region_data(4, 4, flags, dt_selector_2_bits(), 2, writer.into_bytes());
        let dict = dictionary_segment(1, vec![solid_symbol(1, 1)]);
        let segment = text_region_segment(2, 1);

        let image =
            decode_huffman_text_region_segment(&segment, &data, &[dict]).expect("decode region");
        assert_eq!(lit_pixels(&image), vec![(1, 0), (2, 1)]);
    }

    #[test]
    fn transposed_text_region_uses_ti_si_coordinate_order() {
        let (fs_table, _, dt_table) = text_region_tables();
        let mut writer = BitWriter::new();
        push_single_symbol_id_table(&mut writer);
        push_huffman_result(&mut writer, &dt_table, HuffmanValue::Value(1));
        push_huffman_result(&mut writer, &dt_table, HuffmanValue::Value(1));
        push_huffman_result(&mut writer, &fs_table, HuffmanValue::Value(3));
        writer.push_bits(0, 1);
        writer.align_to_byte();

        let flags = TextRegionFlagBits::SBHUFF.bits()
            | TextRegionFlagBits::TRANSPOSED.bits()
            | top_left_corner_bits();
        let data =
            build_text_region_data(5, 5, flags, dt_selector_2_bits(), 1, writer.into_bytes());
        let dict = dictionary_segment(1, vec![solid_symbol(1, 1)]);
        let segment = text_region_segment(2, 1);

        let image =
            decode_huffman_text_region_segment(&segment, &data, &[dict]).expect("decode region");
        assert_eq!(lit_pixels(&image), vec![(0, 3)]);
    }

    #[test]
    fn sbdefpixel_initializes_region_before_composition() {
        let (fs_table, _, dt_table) = text_region_tables();
        let mut writer = BitWriter::new();
        push_single_symbol_id_table(&mut writer);
        push_huffman_result(&mut writer, &dt_table, HuffmanValue::Value(1));
        push_huffman_result(&mut writer, &dt_table, HuffmanValue::Value(1));
        push_huffman_result(&mut writer, &fs_table, HuffmanValue::Value(1));
        writer.push_bits(0, 1);
        writer.align_to_byte();

        let flags = TextRegionFlagBits::SBHUFF.bits()
            | TextRegionFlagBits::SBDEFPIXEL.bits()
            | top_left_corner_bits();
        let data =
            build_text_region_data(4, 3, flags, dt_selector_2_bits(), 1, writer.into_bytes());
        let dict = dictionary_segment(1, vec![JBig2Image::new(1, 1)]);
        let segment = text_region_segment(2, 1);

        let image =
            decode_huffman_text_region_segment(&segment, &data, &[dict]).expect("decode region");
        assert_eq!(lit_pixels(&image).len(), 12);
    }

    #[test]
    fn sbrtemplate_without_refinement_is_accepted_for_huffman_text_regions() {
        let (fs_table, _, dt_table) = text_region_tables();
        let mut writer = BitWriter::new();
        push_single_symbol_id_table(&mut writer);
        push_huffman_result(&mut writer, &dt_table, HuffmanValue::Value(1));
        push_huffman_result(&mut writer, &dt_table, HuffmanValue::Value(1));
        push_huffman_result(&mut writer, &fs_table, HuffmanValue::Value(1));
        writer.push_bits(0, 1);
        writer.align_to_byte();

        let flags = TextRegionFlagBits::SBHUFF.bits()
            | TextRegionFlagBits::SBRTEMPLATE.bits()
            | top_left_corner_bits();
        let data =
            build_text_region_data(2, 2, flags, dt_selector_2_bits(), 1, writer.into_bytes());
        let parsed = ParsedTextRegion::try_from(data.as_slice()).expect("parsed");
        parsed
            .validate_supported_huffman_text_region()
            .expect("supported");
        assert!(parsed.flags.contains(TextRegionFlagBits::SBRTEMPLATE));

        let dict = dictionary_segment(1, vec![solid_symbol(1, 1)]);
        let segment = text_region_segment(2, 1);
        let image =
            decode_huffman_text_region_segment(&segment, &data, &[dict]).expect("decode region");
        assert_eq!(lit_pixels(&image), vec![(1, 0)]);
    }

    #[test]
    fn compute_symbol_placement_is_available_to_decode_fixtures() {
        let placement = TextRegionGeometry::new(false, TextRegionRefCorner::BottomRight)
            .placement_for(0, 0, 2, 2)
            .expect("placement");
        assert_eq!((placement.x, placement.y), (-1, -1));
    }

    #[test]
    fn sbrefine_flag_zero_still_places_the_original_symbol() {
        let (fs_table, _, dt_table) = text_region_tables();
        let mut writer = BitWriter::new();
        push_single_symbol_id_table(&mut writer);
        push_huffman_result(&mut writer, &dt_table, HuffmanValue::Value(1));
        push_huffman_result(&mut writer, &dt_table, HuffmanValue::Value(1));
        push_huffman_result(&mut writer, &fs_table, HuffmanValue::Value(1));
        writer.push_bits(0, 1);
        writer.align_to_byte();

        let flags = TextRegionFlagBits::SBHUFF.bits()
            | TextRegionFlagBits::SBREFINE.bits()
            | TextRegionFlagBits::SBRTEMPLATE.bits()
            | top_left_corner_bits();
        let data =
            build_text_region_data(4, 4, flags, dt_selector_2_bits(), 1, writer.into_bytes());
        let dict = dictionary_segment(1, vec![solid_symbol(1, 1)]);
        let segment = text_region_segment(2, 1);

        let image =
            decode_huffman_text_region_segment(&segment, &data, &[dict]).expect("decode region");
        assert_eq!(lit_pixels(&image), vec![(1, 0)]);
    }

    #[test]
    fn sbrefine_with_standard_tables_is_accepted_for_huffman_text_regions() {
        let mut writer = BitWriter::new();
        push_single_symbol_id_table(&mut writer);
        writer.align_to_byte();

        let flags = TextRegionFlagBits::SBHUFF.bits()
            | TextRegionFlagBits::SBREFINE.bits()
            | TextRegionFlagBits::SBRTEMPLATE.bits()
            | top_left_corner_bits();
        let data =
            build_text_region_data(4, 4, flags, dt_selector_2_bits(), 1, writer.into_bytes());
        let parsed = ParsedTextRegion::try_from(data.as_slice()).expect("parsed");
        let dict = dictionary_segment(1, vec![solid_symbol(1, 1)]);
        let segment = text_region_segment(2, 1);
        let referred_segments = [dict];
        let mut stream = BitReader::new(&[]);
        let context = SegmentDecodeContext::new(&segment, &mut stream, 0, &referred_segments, &[]);
        let context =
            super::HuffmanTextRegionDecodeContext::new(&context, parsed).expect("context");

        assert!(context.refinement_tables.is_some());
        assert!(
            context
                .shared
                .parsed()
                .flags
                .contains(TextRegionFlagBits::SBREFINE)
        );
    }

    #[test]
    fn sbrefine_with_custom_rsize_table_is_accepted_for_huffman_text_regions() {
        let (fs_table, _, dt_table) = text_region_tables();
        let rdw_table = text_region_refinement_standard_decoder(0).expect("rdw");
        let rdh_table = text_region_refinement_standard_decoder(0).expect("rdh");
        let rdx_table = text_region_refinement_standard_decoder(0).expect("rdx");
        let rdy_table = text_region_refinement_standard_decoder(0).expect("rdy");
        let mut writer = BitWriter::new();
        push_single_symbol_id_table(&mut writer);
        let _ =
            push_first_encodable_value(&mut writer, &dt_table, &[1, 0, 2, 3]).expect("dt value");
        let _ =
            push_first_encodable_value(&mut writer, &fs_table, &[1, 0, 2, 3]).expect("fs value");
        writer.push_bits(1, 1);
        let _ =
            push_first_encodable_value(&mut writer, &rdw_table, &[0, 1, 2, 3]).expect("rdw value");
        let _ =
            push_first_encodable_value(&mut writer, &rdh_table, &[0, 1, 2, 3]).expect("rdh value");
        let _ =
            push_first_encodable_value(&mut writer, &rdx_table, &[0, 1, 2, 3]).expect("rdx value");
        let _ =
            push_first_encodable_value(&mut writer, &rdy_table, &[0, 1, 2, 3]).expect("rdy value");
        writer.push_bits(0, 1);
        writer.align_to_byte();
        writer.push_bytes(&[0x00, 0x00, 0x00, 0x00]);

        let flags = TextRegionFlagBits::SBHUFF.bits()
            | TextRegionFlagBits::SBREFINE.bits()
            | TextRegionFlagBits::SBRTEMPLATE.bits()
            | top_left_corner_bits();
        let data = build_text_region_data(4, 4, flags, 0x4000, 1, writer.into_bytes());
        let parsed = ParsedTextRegion::try_from(data.as_slice()).expect("parsed");
        let dict = dictionary_segment(1, vec![solid_symbol(1, 1)]);
        let custom_rsize = huffman_table_segment(2, single_value_custom_table(4));
        let segment = text_region_segment_with_references(3, vec![1, 2]);
        let referred_segments = [dict, custom_rsize];
        let mut stream = BitReader::new(&[]);
        let context = SegmentDecodeContext::new(&segment, &mut stream, 0, &referred_segments, &[]);
        let image = decode_huffman_text_region(&context, parsed).expect("decode region");

        assert_eq!((image.width(), image.height()), (4, 4));
    }

    #[test]
    fn refined_huffman_text_region_decodes_with_full_refinement_size_body() {
        let (fs_table, _, dt_table) = text_region_tables();
        let rdw_table = text_region_refinement_standard_decoder(0).expect("rdw");
        let rdh_table = text_region_refinement_standard_decoder(0).expect("rdh");
        let rdx_table = text_region_refinement_standard_decoder(0).expect("rdx");
        let rdy_table = text_region_refinement_standard_decoder(0).expect("rdy");
        let rsize_table =
            StandardHuffmanDecoder::new(crate::huffman::STANDARD_TABLE_B1).expect("rsize");
        let mut writer = BitWriter::new();
        push_single_symbol_id_table(&mut writer);
        let _ =
            push_first_encodable_value(&mut writer, &dt_table, &[1, 0, 2, 3]).expect("dt value");
        let _ =
            push_first_encodable_value(&mut writer, &fs_table, &[1, 0, 2, 3]).expect("fs value");
        writer.push_bits(0, 1);
        let _ =
            push_first_encodable_value(&mut writer, &rdw_table, &[0, 1, 2, 3]).expect("rdw value");
        let _ =
            push_first_encodable_value(&mut writer, &rdh_table, &[0, 1, 2, 3]).expect("rdh value");
        let _ =
            push_first_encodable_value(&mut writer, &rdx_table, &[0, 1, 2, 3]).expect("rdx value");
        let _ =
            push_first_encodable_value(&mut writer, &rdy_table, &[0, 1, 2, 3]).expect("rdy value");
        push_huffman_result(&mut writer, &rsize_table, HuffmanValue::Value(4));
        writer.push_bytes(&[0x00, 0x00, 0x00, 0x00]);

        let flags = TextRegionFlagBits::SBHUFF.bits()
            | TextRegionFlagBits::SBREFINE.bits()
            | TextRegionFlagBits::SBRTEMPLATE.bits()
            | top_left_corner_bits();
        let huffman_flags = 0x0000;
        let data = build_text_region_data(399, 400, flags, huffman_flags, 1, writer.into_bytes());
        let dict = dictionary_segment(1, vec![solid_symbol(1, 1)]);
        let segment = text_region_segment(2, 1);

        let image =
            decode_huffman_text_region_segment(&segment, &data, &[dict]).expect("decode region");
        assert_eq!((image.width(), image.height()), (399, 400));
        assert_eq!(image.to_tight_bytes().len(), 20_000);
    }

    #[test]
    fn sample_bitmap_symbol_symhuffrefine_textrefine_decodes() {
        let decoded =
            decode_segments(textrefine_jbig2_stream(), Some((399, 400))).expect("JBIG2 sample");

        assert_eq!((decoded.page.width(), decoded.page.height()), (399, 400));
    }

    #[test]
    fn sample_bitmap_symbol_symhuffrefine_textrefine_builds_text_context() {
        let (segments, text_segment, text_body) = decode_sample_segment_dictionaries();
        let parsed = ParsedTextRegion::try_from(text_body.as_slice()).expect("parsed text");
        let mut stream = BitReader::new(&[]);
        let context = SegmentDecodeContext::new(&text_segment, &mut stream, 0, &segments, &[]);
        let decode_context =
            super::HuffmanTextRegionDecodeContext::new(&context, parsed).expect("context");

        assert_eq!(decode_context.shared.symbols_len(), 11);
        assert!(!decode_context.symbol_id_table.codes().is_empty());
    }
}
