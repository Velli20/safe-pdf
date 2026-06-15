//! JBIG2 Huffman text-region decoder.

use crate::{
    arith_decoder::JBig2ArithDecoder,
    compose_op::ComposeOp,
    error::Jbig2Error,
    generic_refinement_region::{
        GenericRefinementRegionDecode, RefinementAdaptiveTemplate, RefinementTemplate,
    },
    huffman::{
        CustomHuffmanDecoder, HuffmanDecoder, HuffmanTableSelection, HuffmanValue,
        StandardHuffmanDecoder, SymbolIdHuffmanTable, decode_symbol_id,
        decode_symbol_id_huffman_table, text_region_refinement_standard_decoder,
        text_region_rsize_standard_decoder,
    },
    image::JBig2Image,
    segment_context::SegmentDecodeContext,
    text_region::{
        bitmap::initialized_region, flags::TextRegionFlagBits, geometry::TextRegionGeometry,
        huffman_flags::TextRegionHuffmanFlags, parser::ParsedTextRegion,
        state::TextRegionDecodeState,
    },
    util::{INTEGER_CONVERSION_OVERFLOW, ceil_log2, refined_dimension, required_huffman_value},
};
use pdf_utils::BitReader;

const TEXT_REGION_BODY: &str = "text region body";
const TEXT_REGION_STRIP_INDEX: &str = "text region strip index";
const TEXT_REGION_SYMBOL_ID: &str = "text region symbol id";
const TEXT_REGION_REFINEMENT_TABLES: &str = "text-region refinement Huffman tables";
const TEXT_REGION_REFINEMENT_FLAG: &str = "text region refinement flag";
const TEXT_REGION_REFINEMENT_WIDTH: &str = "text region refinement width";
const TEXT_REGION_REFINEMENT_HEIGHT: &str = "text region refinement height";

/// Parsed and resolved state needed by the Huffman text-region procedure.
///
/// ITU-T T.88 | ISO/IEC 14492 section 6.4.5 uses the parsed segment header,
/// referred symbol dictionaries, three standard Huffman tables, and a symbol
/// ID table to place each symbol instance into the region bitmap.
struct TextRegionDecodeContext<'a> {
    parsed: ParsedTextRegion<'a>,
    symbols: Vec<JBig2Image>,
    fs_table: HuffmanDecoder,
    ds_table: HuffmanDecoder,
    dt_table: HuffmanDecoder,
    symbol_id_table: SymbolIdHuffmanTable,
    refinement_tables: Option<TextRegionRefinementTables>,
    compose_op: ComposeOp,
    body: &'a [u8],
}

/// Decoded Huffman symbol-instance coordinates and symbol ID.
///
/// ITU-T T.88 | ISO/IEC 14492 section 6.4.5 step 3(c) derives `CURS`, `TI`,
/// and the symbol ID before bitmap lookup and composition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DecodedSymbolInstance {
    curs: i64,
    ti: i64,
    symbol_id: usize,
    refined: bool,
}

/// Standard Huffman tables used by refinement-coded text regions.
#[derive(Debug, Clone)]
struct TextRegionRefinementTables {
    rsize_table: StandardHuffmanDecoder,
    rdw_table: StandardHuffmanDecoder,
    rdh_table: StandardHuffmanDecoder,
    rdx_table: StandardHuffmanDecoder,
    rdy_table: StandardHuffmanDecoder,
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

impl<'a> TextRegionDecodeContext<'a> {
    /// Resolve dictionaries, Huffman tables, and the symbol-ID table.
    ///
    /// ITU-T T.88 | ISO/IEC 14492 section 7.4.3.1.2 defines the table
    /// selectors. Section 6.4.10 defines symbol-ID decoding.
    fn new(
        context: &SegmentDecodeContext<'_, '_, '_, '_, '_>,
        parsed: ParsedTextRegion<'a>,
    ) -> Result<Self, Jbig2Error> {
        parsed.validate_supported_huffman_text_region()?;

        let symbols = context.referred_symbol_images()?;
        if symbols.is_empty() {
            return Err(Jbig2Error::MissingSegment);
        }
        let huffman_flags = parsed
            .huffman_flags
            .ok_or(Jbig2Error::InvalidState("text region Huffman flags"))?;
        let custom_tables = referred_huffman_tables(context)?;
        let mut custom_index = 0usize;
        let fs_table = text_region_table(
            HuffmanTableSelection::TextRegionFs(huffman_flags.fs_selector),
            &custom_tables,
            &mut custom_index,
        )?;
        let ds_table = text_region_table(
            HuffmanTableSelection::TextRegionDs(huffman_flags.ds_selector),
            &custom_tables,
            &mut custom_index,
        )?;
        let dt_table = text_region_table(
            HuffmanTableSelection::TextRegionDt(huffman_flags.dt_selector),
            &custom_tables,
            &mut custom_index,
        )?;
        let mut body_reader = BitReader::new(parsed.body);
        let symbol_id_table = decode_symbol_id_huffman_table(&mut body_reader, symbols.len())?;
        body_reader.align_to_byte_boundary();
        let body = body_reader
            .remaining_from_byte()
            .ok_or(Jbig2Error::Truncated(TEXT_REGION_BODY))?;
        let refinement_tables = if parsed.flags.contains(TextRegionFlagBits::SBREFINE) {
            Some(TextRegionRefinementTables::new(huffman_flags)?)
        } else {
            None
        };

        let compose_op = ComposeOp::from(parsed.flags.sbcombop_bits());

        Ok(Self {
            parsed,
            symbols,
            fs_table,
            ds_table,
            dt_table,
            symbol_id_table,
            refinement_tables,
            compose_op,
            body,
        })
    }
}

fn text_region_table(
    selection: HuffmanTableSelection,
    custom_tables: &[CustomHuffmanDecoder],
    custom_index: &mut usize,
) -> Result<HuffmanDecoder, Jbig2Error> {
    match selection {
        HuffmanTableSelection::TextRegionFs(3)
        | HuffmanTableSelection::TextRegionDs(3)
        | HuffmanTableSelection::TextRegionDt(3) => {
            let table = custom_tables
                .get(*custom_index)
                .cloned()
                .ok_or(Jbig2Error::MissingSegment)?;
            *custom_index = custom_index
                .checked_add(1)
                .ok_or(Jbig2Error::Overflow("Huffman table index overflow"))?;
            Ok(HuffmanDecoder::Custom(table))
        }
        _ => selection.standard_decoder().map(HuffmanDecoder::Standard),
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
    /// Resolve the standard Huffman tables used by refinement-coded instances.
    fn new(huffman_flags: TextRegionHuffmanFlags) -> Result<Self, Jbig2Error> {
        let rsize_table = text_region_rsize_standard_decoder(huffman_flags.rsize_custom)?;
        let rdw_table = text_region_refinement_standard_decoder(huffman_flags.rdw_selector)?;
        let rdh_table = text_region_refinement_standard_decoder(huffman_flags.rdh_selector)?;
        let rdx_table = text_region_refinement_standard_decoder(huffman_flags.rdx_selector)?;
        let rdy_table = text_region_refinement_standard_decoder(huffman_flags.rdy_selector)?;

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
    context: TextRegionDecodeContext<'a>,
    body_reader: BitReader<'a>,
    region: JBig2Image,
    state: TextRegionDecodeState,
    geometry: TextRegionGeometry,
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
        let context = TextRegionDecodeContext::new(context, parsed)?;
        let mut body_reader = BitReader::new(context.body);
        let initial_stript = required_huffman_value(context.dt_table.decode(&mut body_reader)?)?;
        let state = TextRegionDecodeState::from_initial_delta(
            initial_stript,
            context.parsed.flags.sbstrips(),
        )?;
        let region = initialized_region(&context.parsed)?;
        let geometry = TextRegionGeometry::from_flags(context.parsed.flags)?;

        Ok(Self {
            context,
            body_reader,
            region,
            state,
            geometry,
        })
    }

    /// Decode all symbol instances into the text-region bitmap.
    ///
    /// This implements ITU-T T.88 | ISO/IEC 14492 section 6.4.5 step 3 until
    /// `NINSTANCES` reaches `SBNUMINSTANCES`.
    fn decode(mut self) -> Result<JBig2Image, Jbig2Error> {
        while !self.state.is_complete(self.context.parsed.symbol_instances) {
            self.decode_strip()?;
        }

        Ok(self.region)
    }

    /// Decode one strip of Huffman symbol instances.
    ///
    /// ITU-T T.88 | ISO/IEC 14492 section 6.4.5 step 3(b) starts a strip and
    /// step 3(c) decodes instances until Huffman OOB or completion.
    fn decode_strip(&mut self) -> Result<(), Jbig2Error> {
        let stript = self.decode_next_strip_header()?;

        while let Some(instance) = self.decode_strip_symbol_position(stript)? {
            self.draw_instance(instance)?;
            if self.state.is_complete(self.context.parsed.symbol_instances) {
                break;
            }
        }

        Ok(())
    }

    /// Decode the next strip `DT` header.
    ///
    /// ITU-T T.88 | ISO/IEC 14492 sections 6.4.5 step 3(b) and 6.4.6 define
    /// strip delta decoding and `STRIPT` advancement.
    fn decode_next_strip_header(&mut self) -> Result<i64, Jbig2Error> {
        let delta_t = required_huffman_value(self.context.dt_table.decode(&mut self.body_reader)?)?;
        self.state
            .advance_strip(delta_t, self.context.parsed.flags.sbstrips())
    }

    /// Decode the next symbol-instance position within the current strip.
    ///
    /// ITU-T T.88 | ISO/IEC 14492 section 6.4.5 step 3(c), section 6.4.7 for
    /// first-symbol `S`, section 6.4.8 for subsequent `S`, section 6.4.9 for
    /// `T`, and section 6.4.10 for symbol ID.
    fn decode_strip_symbol_position(
        &mut self,
        stript: i64,
    ) -> Result<Option<DecodedSymbolInstance>, Jbig2Error> {
        if self.state.strip_first_symbol_pending() {
            let dfs = required_huffman_value(self.context.fs_table.decode(&mut self.body_reader)?)?;
            self.state.consume_huffman_first_s(dfs)?;
        } else {
            match self.context.ds_table.decode(&mut self.body_reader)? {
                HuffmanValue::OutOfBand => return Ok(None),
                HuffmanValue::Value(delta_s) => self
                    .state
                    .consume_huffman_delta_s(delta_s, self.context.parsed.flags.sbdsoffset())?,
            }
        }

        let sbstrips = self.context.parsed.flags.sbstrips();
        let curt = if sbstrips == 1 {
            0
        } else {
            let bits = ceil_log2(usize::from(sbstrips))?;
            i64::from(
                self.body_reader
                    .read_bits(bits)
                    .ok_or(Jbig2Error::Truncated(TEXT_REGION_STRIP_INDEX))?,
            )
        };
        let ti = stript
            .checked_add(curt)
            .ok_or(Jbig2Error::Overflow(INTEGER_CONVERSION_OVERFLOW))?;
        let symbol_id = decode_symbol_id(&mut self.body_reader, &self.context.symbol_id_table)?;
        let refined = if self
            .context
            .parsed
            .flags
            .contains(TextRegionFlagBits::SBREFINE)
        {
            self.body_reader
                .next_bit()
                .ok_or(Jbig2Error::Truncated(TEXT_REGION_REFINEMENT_FLAG))?
        } else {
            false
        };

        Ok(Some(DecodedSymbolInstance {
            curs: self.state.strip_curs(),
            ti,
            symbol_id,
            refined,
        }))
    }

    /// Compose one decoded symbol instance into the region bitmap.
    ///
    /// ITU-T T.88 | ISO/IEC 14492 section 6.4.5 step 3(c) looks up the symbol
    /// bitmap, computes placement, composes it, and increments `NINSTANCES`.
    fn draw_instance(&mut self, instance: DecodedSymbolInstance) -> Result<(), Jbig2Error> {
        let symbol = self
            .context
            .symbols
            .get(instance.symbol_id)
            .ok_or(Jbig2Error::InvalidState(TEXT_REGION_SYMBOL_ID))?;
        if instance.refined {
            return self.draw_refined_instance(symbol.clone(), instance.curs, instance.ti);
        }
        let curs = self.geometry.adjust_curs_before_placement(
            instance.curs,
            symbol.width(),
            symbol.height(),
        )?;
        let placement =
            self.geometry
                .placement_for(curs, instance.ti, symbol.width(), symbol.height())?;
        symbol.compose_clipped_to(
            &mut self.region,
            placement.x,
            placement.y,
            self.context.compose_op,
        );
        self.state.record_instance()?;
        Ok(())
    }

    /// Decode, compose, and advance one refinement-coded symbol instance.
    fn draw_refined_instance(
        &mut self,
        reference: JBig2Image,
        curs: i64,
        ti: i64,
    ) -> Result<(), Jbig2Error> {
        let tables = self
            .context
            .refinement_tables
            .as_ref()
            .ok_or(Jbig2Error::InvalidState(TEXT_REGION_REFINEMENT_TABLES))?;
        let delta_width = required_huffman_value(tables.rdw_table.decode(&mut self.body_reader)?)?;
        let delta_height = required_huffman_value(tables.rdh_table.decode(&mut self.body_reader)?)?;
        let delta_x = required_huffman_value(tables.rdx_table.decode(&mut self.body_reader)?)?;
        let delta_y = required_huffman_value(tables.rdy_table.decode(&mut self.body_reader)?)?;
        let refinement_size =
            required_huffman_value(tables.rsize_table.decode(&mut self.body_reader)?)?;
        let width =
            refined_dimension(reference.width(), delta_width, TEXT_REGION_REFINEMENT_WIDTH)?;
        let height = refined_dimension(
            reference.height(),
            delta_height,
            TEXT_REGION_REFINEMENT_HEIGHT,
        )?;
        let template = RefinementTemplate::from_flag(
            self.context
                .parsed
                .flags
                .contains(TextRegionFlagBits::SBRTEMPLATE),
        );
        let at = self
            .context
            .parsed
            .refinement_at
            .unwrap_or_else(|| RefinementAdaptiveTemplate::default_for(template));
        let reference_dx = delta_width
            .checked_shr(2)
            .and_then(|value| value.checked_add(delta_x))
            .ok_or(Jbig2Error::Overflow(INTEGER_CONVERSION_OVERFLOW))?;
        let reference_dy = delta_height
            .checked_shr(2)
            .and_then(|value| value.checked_add(delta_y))
            .ok_or(Jbig2Error::Overflow(INTEGER_CONVERSION_OVERFLOW))?;
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
            GenericRefinementRegionDecode::new(
                width,
                height,
                template,
                false,
                at,
                reference_dx,
                reference_dy,
            )
            .decode(&reference, &mut arith_decoder)?
        };
        let placed_curs =
            self.geometry
                .adjust_curs_before_placement(curs, image.width(), image.height())?;
        let placement =
            self.geometry
                .placement_for(placed_curs, ti, image.width(), image.height())?;
        image.compose_clipped_to(
            &mut self.region,
            placement.x,
            placement.y,
            self.context.compose_op,
        );
        self.body_reader.align_to_byte_boundary();
        self.geometry
            .advance_curs_after_placement(placed_curs, image.width(), image.height())?;
        self.state.record_instance()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use pdf_utils::BitReader;

    use crate::{
        error::Jbig2Error,
        huffman::{
            HuffmanTableSelection, HuffmanValue, StandardHuffmanDecoder,
            test_support::bits_for_value, text_region_refinement_standard_decoder,
            text_region_rsize_standard_decoder,
        },
        image::JBig2Image,
        segment::{JBig2SegmentResult, ParsedSegment},
        segment_context::SegmentDecodeContext,
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
        let context = super::TextRegionDecodeContext::new(&context, parsed).expect("context");

        assert!(context.refinement_tables.is_some());
        assert!(context.parsed.flags.contains(TextRegionFlagBits::SBREFINE));
    }

    #[test]
    fn sbrefine_with_custom_refinement_tables_is_unsupported() {
        let (fs_table, _, dt_table) = text_region_tables();
        let mut writer = BitWriter::new();
        push_single_symbol_id_table(&mut writer);
        push_huffman_result(&mut writer, &dt_table, HuffmanValue::Value(1));
        push_huffman_result(&mut writer, &dt_table, HuffmanValue::Value(1));
        push_huffman_result(&mut writer, &fs_table, HuffmanValue::Value(1));
        writer.push_bits(1, 1);
        writer.align_to_byte();

        let flags = TextRegionFlagBits::SBHUFF.bits()
            | TextRegionFlagBits::SBREFINE.bits()
            | TextRegionFlagBits::SBRTEMPLATE.bits()
            | top_left_corner_bits();
        let data = build_text_region_data(4, 4, flags, 0x0080, 1, writer.into_bytes());
        let parsed = ParsedTextRegion::try_from(data.as_slice()).expect("parsed");
        let dict = dictionary_segment(1, vec![solid_symbol(1, 1)]);
        let segment = text_region_segment(2, 1);
        let referred_segments = [dict];
        let mut stream = BitReader::new(&[]);
        let context = SegmentDecodeContext::new(&segment, &mut stream, 0, &referred_segments, &[]);
        let err = decode_huffman_text_region(&context, parsed).expect_err("unsupported");
        assert!(matches!(err, Jbig2Error::UnsupportedFeature(_)));
    }

    #[test]
    fn refined_huffman_text_region_decodes_with_full_refinement_size_body() {
        let (fs_table, _, dt_table) = text_region_tables();
        let rdw_table = text_region_refinement_standard_decoder(0).expect("rdw");
        let rdh_table = text_region_refinement_standard_decoder(0).expect("rdh");
        let rdx_table = text_region_refinement_standard_decoder(0).expect("rdx");
        let rdy_table = text_region_refinement_standard_decoder(0).expect("rdy");
        let rsize_table = text_region_rsize_standard_decoder(false).expect("rsize");
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
}
