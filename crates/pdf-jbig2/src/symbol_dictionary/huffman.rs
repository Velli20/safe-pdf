use pdf_utils::BitReader;

use crate::{
    arith_decoder::JBig2ArithDecoder,
    error::Jbig2Error,
    generic_refinement_region::{
        GenericRefinementRegionDecode, RefinementAdaptiveTemplate, RefinementTemplate,
    },
    generic_region::decode_mmr_region,
    huffman::{
        CustomHuffmanDecoder, CustomHuffmanTableCursor, HuffmanDecoder, HuffmanTableSelection,
        HuffmanValue, STANDARD_TABLE_B1, STANDARD_TABLE_B15, StandardHuffmanDecoder,
        text_region_refinement_standard_decoder,
    },
    image::JBig2Image,
    symbol_dictionary::{
        aggregate::decode_aggregate_symbol_instances,
        collective_bitmap::append_collective_bitmap_symbols,
        export::total_symbol_count,
        flags::SymbolDictionaryFlagBits,
        header::ParsedSymbolDictionaryHeader,
        refinement::{
            aggregate_refinement_geometry, aggregate_refinement_params,
            decode_refinement_symbol_from_deltas,
        },
    },
    text_region::state::TextRegionDecodeState,
    util::{ceil_log2, i32_to_u16, i32_to_usize, usize_to_u16},
};

const COLLECTIVE_BITMAP: &str = "collective bitmap";
const COLLECTIVE_BITMAP_SIZE: &str = "collective bitmap size";
const COLLECTIVE_BITMAP_WIDTH: &str = "collective bitmap width";
const HUFFMAN_REFINEMENT_BITMAP: &str = "Huffman symbol dictionary refinement bitmap";
const HUFFMAN_REFINEMENT_BITMAP_SIZE: &str = "Huffman symbol dictionary refinement bitmap size";
const DEFERRED_HUFFMAN_REFINEMENT_BITMAP: &str =
    "deferred Huffman symbol dictionary refinement bitmap";
const CAPTURED_HUFFMAN_REFINEMENT_BITMAP: &str =
    "captured Huffman symbol dictionary refinement bitmap";
const HUFFMAN_SYMBOL_HEIGHT: &str = "Huffman symbol height";
const HUFFMAN_SYMBOL_WIDTH: &str = "Huffman symbol width";
const REFINEMENT_SYMBOL_HEIGHT: &str = "refinement symbol height";
const REFINEMENT_SYMBOL_WIDTH: &str = "refinement symbol width";
const IMAGE_DIMENSIONS_OVERFLOW: &str = "image dimensions overflow";
const INTEGER_CONVERSION_OVERFLOW: &str = "integer conversion overflow";
const REFINEMENT_AGGREGATE_INSTANCES: &str = "refinement aggregate instances";
const SYMBOL_DICTIONARY_WIDTH_RUN: &str = "symbol dictionary width run";
const SYMBOL_DICTIONARY_REFINEMENT_SYMBOL_ID: &str = "symbol dictionary refinement symbol id";
const AGGREGATE_SYMBOL_DICTIONARY_REFINEMENT_SYMBOL_ID: &str =
    "aggregate symbol dictionary refinement symbol id";
const DEFERRED_AGGREGATE_SYMBOL_REFERENCE: &str =
    "deferred aggregate symbol dictionary refinement reference";

/// Huffman-coded symbol dictionary decoder.
///
/// ITU-T T.88 / ISO/IEC 14492 section 7.4.2 uses the `SDHUFFDH`,
/// `SDHUFFDW`, and `SDHUFFBMSIZE` selectors to choose standard Huffman tables
/// from Annex B for Huffman-coded symbol dictionaries.
pub(super) struct HuffmanSymbolDictionaryDecoder<'stream, 'data> {
    stream: &'stream mut BitReader<'data>,
    header: ParsedSymbolDictionaryHeader,
    dh_table: HuffmanDecoder,
    dw_table: HuffmanDecoder,
    bmsize_table: HuffmanDecoder,
    agginst_table: HuffmanDecoder,
    refinement_table: StandardHuffmanDecoder,
    aggregate_fs_table: StandardHuffmanDecoder,
    aggregate_ds_table: StandardHuffmanDecoder,
    aggregate_dt_table: StandardHuffmanDecoder,
    aggregate_refinement_table: StandardHuffmanDecoder,
    aggregate_rsize_table: StandardHuffmanDecoder,
    export_table: StandardHuffmanDecoder,
}

impl<'stream, 'data> HuffmanSymbolDictionaryDecoder<'stream, 'data> {
    /// Build decoders for the standard Huffman tables selected by the header flags.
    ///
    /// ITU-T T.88 / ISO/IEC 14492 section 7.4.2.1.1 defines the table selector
    /// fields. This implementation supports only the standard Annex B tables.
    pub(super) fn new(
        stream: &'stream mut BitReader<'data>,
        header: ParsedSymbolDictionaryHeader,
        custom_tables: Vec<CustomHuffmanDecoder>,
    ) -> Result<Self, Jbig2Error> {
        let flags = header.flags;
        let mut custom_tables = CustomHuffmanTableCursor::new(custom_tables);
        let dh_table = custom_tables
            .symbol_dictionary_table(HuffmanTableSelection::SymbolDictionaryDh(flags.sdhuffdh()))?;
        let dw_table = custom_tables
            .symbol_dictionary_table(HuffmanTableSelection::SymbolDictionaryDw(flags.sdhuffdw()))?;
        let bmsize_table = if flags.sdhuffbmsize() {
            custom_tables.next_decoder()?
        } else {
            HuffmanDecoder::Standard(StandardHuffmanDecoder::new(STANDARD_TABLE_B1)?)
        };
        let agginst_table = if flags.sdhuffagginst() {
            custom_tables.next_decoder()?
        } else {
            HuffmanDecoder::Standard(StandardHuffmanDecoder::new(STANDARD_TABLE_B1)?)
        };
        let refinement_table = StandardHuffmanDecoder::new(STANDARD_TABLE_B15)?;
        let aggregate_fs_table = HuffmanTableSelection::TextRegionFs(0).standard_decoder()?;
        let aggregate_ds_table = HuffmanTableSelection::TextRegionDs(0).standard_decoder()?;
        let aggregate_dt_table = HuffmanTableSelection::TextRegionDt(0).standard_decoder()?;
        let aggregate_refinement_table = text_region_refinement_standard_decoder(0)?;
        let aggregate_rsize_table = StandardHuffmanDecoder::new(STANDARD_TABLE_B1)?;
        let export_table = StandardHuffmanDecoder::new(STANDARD_TABLE_B1)?;

        Ok(Self {
            stream,
            header,
            dh_table,
            dw_table,
            bmsize_table,
            agginst_table,
            refinement_table,
            aggregate_fs_table,
            aggregate_ds_table,
            aggregate_dt_table,
            aggregate_refinement_table,
            aggregate_rsize_table,
            export_table,
        })
    }

    /// Decode all new symbols declared by a Huffman-coded symbol dictionary.
    ///
    /// ITU-T T.88 / ISO/IEC 14492 section 7.4.2 groups Huffman-coded symbols
    /// by height. Each height run carries width deltas followed by a
    /// horizontally concatenated collective bitmap for that run.
    pub(super) fn decode_new_symbols(
        &mut self,
        input_symbols: &[JBig2Image],
        num_new_symbols: usize,
    ) -> Result<Vec<JBig2Image>, Jbig2Error> {
        if self
            .header
            .flags
            .contains(SymbolDictionaryFlagBits::SDREFAGG)
        {
            return self.decode_refined_symbols(input_symbols, num_new_symbols);
        }
        let mut new_symbols = Vec::with_capacity(num_new_symbols);
        let mut height = 0i32;

        while new_symbols.len() < num_new_symbols {
            let delta_height = self.dh_table.decode_value(self.stream)?;
            height = height
                .checked_add(delta_height)
                .ok_or(Jbig2Error::Overflow(INTEGER_CONVERSION_OVERFLOW))?;
            let width_run = self.decode_width_run(num_new_symbols, new_symbols.len())?;
            let height = i32_to_u16(height, HUFFMAN_SYMBOL_HEIGHT)?;
            let collective_bitmap = self.decode_collective_bitmap(width_run.total_width, height)?;
            append_collective_bitmap_symbols(
                &mut new_symbols,
                &collective_bitmap,
                &width_run.widths,
                height,
            )?;
        }

        Ok(new_symbols)
    }

    /// Decode Huffman-coded symbols that use refinement/aggregate syntax.
    ///
    /// ITU-T T.88 / ISO/IEC 14492 section 7.4.2.1.1 uses `SDREFAGG` to switch
    /// from collective bitmaps to symbol refinement decoding.
    fn decode_refined_symbols(
        &mut self,
        input_symbols: &[JBig2Image],
        num_new_symbols: usize,
    ) -> Result<Vec<JBig2Image>, Jbig2Error> {
        let symbol_code_length =
            ceil_log2(input_symbols.len().saturating_add(num_new_symbols))?.max(1);
        let mut pending_symbols = Vec::with_capacity(num_new_symbols);
        let mut height = 0i32;

        while pending_symbols.len() < num_new_symbols {
            let delta_height = self.dh_table.decode_value(self.stream)?;
            height = height
                .checked_add(delta_height)
                .ok_or(Jbig2Error::Overflow(INTEGER_CONVERSION_OVERFLOW))?;
            let height = i32_to_u16(height, HUFFMAN_SYMBOL_HEIGHT)?;
            self.decode_refined_width_run(
                input_symbols,
                &mut pending_symbols,
                num_new_symbols,
                height,
                symbol_code_length,
            )?;
        }

        PendingHuffmanSymbolResolver::new(input_symbols, &pending_symbols).resolve_all()
    }

    /// Decode one refinement width run, consuming each symbol body immediately.
    ///
    /// Unlike collective-bitmap Huffman dictionaries, refinement/aggregate
    /// dictionaries store each symbol's refinement data directly after its
    /// width delta. Reading all widths first would consume bitmap payload as
    /// additional width codes.
    fn decode_refined_width_run(
        &mut self,
        input_symbols: &[JBig2Image],
        pending_symbols: &mut Vec<PendingHuffmanSymbol<'data>>,
        num_new_symbols: usize,
        height: u16,
        symbol_code_length: u8,
    ) -> Result<(), Jbig2Error> {
        let mut symbol_width = 0i32;

        loop {
            match self.dw_table.decode(self.stream)? {
                HuffmanValue::OutOfBand => break,
                HuffmanValue::Value(delta_width) => {
                    symbol_width = symbol_width
                        .checked_add(delta_width)
                        .ok_or(Jbig2Error::Overflow(INTEGER_CONVERSION_OVERFLOW))?;
                    let width = i32_to_u16(symbol_width, HUFFMAN_SYMBOL_WIDTH)?;
                    let symbol = if width == 0 || height == 0 {
                        PendingHuffmanSymbol::Ready(JBig2Image::empty())
                    } else {
                        self.decode_refined_symbol(
                            input_symbols,
                            pending_symbols,
                            width,
                            height,
                            symbol_code_length,
                        )?
                    };
                    pending_symbols.push(symbol);
                    if pending_symbols.len() >= num_new_symbols {
                        break;
                    }
                }
            }
        }

        Ok(())
    }

    /// Decode one Huffman width run and return its symbol widths.
    ///
    /// ITU-T T.88 / ISO/IEC 14492 section 7.4.2 terminates a width run with an
    /// out-of-band code from the selected `SDHUFFDW` table.
    fn decode_width_run(
        &mut self,
        num_new_symbols: usize,
        already_decoded: usize,
    ) -> Result<HuffmanWidthRun, Jbig2Error> {
        let mut symbol_width = 0i32;
        let mut total_width = 0usize;
        let mut widths = Vec::new();

        loop {
            match self.dw_table.decode(self.stream)? {
                HuffmanValue::OutOfBand => break,
                HuffmanValue::Value(delta_width) => {
                    symbol_width = symbol_width
                        .checked_add(delta_width)
                        .ok_or(Jbig2Error::Overflow(INTEGER_CONVERSION_OVERFLOW))?;
                    let width = i32_to_u16(symbol_width, HUFFMAN_SYMBOL_WIDTH)?;
                    widths.push(width);
                    total_width = total_width
                        .checked_add(usize::from(width))
                        .ok_or(Jbig2Error::Overflow(IMAGE_DIMENSIONS_OVERFLOW))?;
                    let decoded_in_run = widths.len();
                    let decoded_total = already_decoded
                        .checked_add(decoded_in_run)
                        .ok_or(Jbig2Error::Overflow(INTEGER_CONVERSION_OVERFLOW))?;
                    if decoded_total > num_new_symbols {
                        return Err(Jbig2Error::InvalidState(SYMBOL_DICTIONARY_WIDTH_RUN));
                    }
                }
            }
        }

        Ok(HuffmanWidthRun {
            widths,
            total_width,
        })
    }

    /// Decode one refinement-coded Huffman symbol.
    fn decode_refined_symbol(
        &mut self,
        input_symbols: &[JBig2Image],
        pending_symbols: &[PendingHuffmanSymbol<'data>],
        width: u16,
        height: u16,
        symbol_code_length: u8,
    ) -> Result<PendingHuffmanSymbol<'data>, Jbig2Error> {
        let ready_symbols = ready_symbol_prefix(pending_symbols);
        let params = aggregate_refinement_params(
            input_symbols,
            &ready_symbols,
            SYMBOL_DICTIONARY_REFINEMENT_SYMBOL_ID,
            symbol_code_length,
            &self.header,
        );
        let aggregate_instances = self.decode_aggregate_instances()?;
        if aggregate_instances > 1 {
            return self.capture_aggregate_refined_symbol(
                input_symbols,
                pending_symbols,
                aggregate_instances,
                width,
                height,
            );
        }
        if aggregate_instances < 0 {
            return Err(Jbig2Error::InvalidState(REFINEMENT_AGGREGATE_INSTANCES));
        }

        let symbol_id = usize::try_from(self.read_raw_bits(symbol_code_length)?)
            .map_err(|_| Jbig2Error::Overflow(INTEGER_CONVERSION_OVERFLOW))?;
        let reference = params.symbols.get(symbol_id)?;
        let delta_x = self.refinement_table.decode_value(self.stream)?;
        let delta_y = self.refinement_table.decode_value(self.stream)?;
        let bitmap_size = self.aggregate_rsize_table.decode_value(self.stream)?;
        let bitmap_size = i32_to_usize(bitmap_size)
            .map_err(|_| Jbig2Error::InvalidState(HUFFMAN_REFINEMENT_BITMAP_SIZE))?;
        let Some(bitmap_body) = self.take_refinement_bitmap_body(bitmap_size)? else {
            return Ok(PendingHuffmanSymbol::Ready(reference.clone()));
        };
        let mut refinement_reader = BitReader::new(bitmap_body);
        let mut decoder = JBig2ArithDecoder::new(&mut refinement_reader);
        GenericRefinementRegionDecode::new(
            width,
            height,
            params.refinement.template,
            false,
            params.refinement.at,
            delta_x,
            delta_y,
        )
        .decode(reference, &mut decoder)
        .map(PendingHuffmanSymbol::Ready)
    }

    /// Capture one aggregate-coded symbol and defer bitmap resolution.
    fn capture_aggregate_refined_symbol(
        &mut self,
        input_symbols: &[JBig2Image],
        pending_symbols: &[PendingHuffmanSymbol<'data>],
        aggregate_instances: i32,
        width: u16,
        height: u16,
    ) -> Result<PendingHuffmanSymbol<'data>, Jbig2Error> {
        let symbol_instances = u32::try_from(aggregate_instances)
            .map_err(|_| Jbig2Error::InvalidState(REFINEMENT_AGGREGATE_INSTANCES))?;
        let aggregate_symbol_count = input_symbols
            .len()
            .checked_add(pending_symbols.len())
            .ok_or(Jbig2Error::Overflow(INTEGER_CONVERSION_OVERFLOW))?;
        let symbol_code_length = ceil_log2(aggregate_symbol_count)?.max(1);
        let refinement = aggregate_refinement_params(
            input_symbols,
            &ready_symbol_prefix(pending_symbols),
            AGGREGATE_SYMBOL_DICTIONARY_REFINEMENT_SYMBOL_ID,
            symbol_code_length,
            &self.header,
        )
        .refinement;
        let start_pos = self.stream.pos();
        let initial_stript = self.aggregate_dt_table.decode_value(self.stream)?;
        let mut state = TextRegionDecodeState::from_initial_delta(initial_stript, 1)?;

        while !state.is_complete(symbol_instances) {
            self.capture_aggregate_symbol_strip(symbol_code_length, &mut state, symbol_instances)?;
        }

        self.stream.align_to_byte_boundary();
        let end_pos = self.stream.pos();
        let start_byte = start_pos / 8;
        let end_byte = end_pos.saturating_add(7) / 8;
        let saved_pos = self.stream.pos();
        self.stream.set_pos(start_byte.saturating_mul(8));
        let body = self
            .stream
            .remaining_from_byte_len(end_byte.saturating_sub(start_byte))
            .ok_or(Jbig2Error::Truncated(HUFFMAN_REFINEMENT_BITMAP))?;
        self.stream.set_pos(saved_pos);

        Ok(PendingHuffmanSymbol::DeferredAggregate(
            DeferredAggregateRefinedSymbol {
                width,
                height,
                body,
                start_bit_offset: start_pos % 8,
                symbol_instances,
                symbol_code_length,
                template: refinement.template,
                at: refinement.at,
            },
        ))
    }

    fn capture_aggregate_symbol_strip(
        &mut self,
        symbol_code_length: u8,
        state: &mut TextRegionDecodeState,
        symbol_instances: u32,
    ) -> Result<(), Jbig2Error> {
        let delta_t = self.aggregate_dt_table.decode_value(self.stream)?;
        state.advance_strip(delta_t, 1)?;
        let delta_first_s = self.aggregate_fs_table.decode_value(self.stream)?;
        state.consume_first_s_delta(delta_first_s)?;

        loop {
            self.read_raw_bits(symbol_code_length)?;
            self.capture_aggregate_symbol_instance()?;
            state.record_instance()?;

            if state.is_complete(symbol_instances) {
                break;
            }
            match self.aggregate_ds_table.decode(self.stream)? {
                HuffmanValue::OutOfBand => break,
                HuffmanValue::Value(_) => {}
            }
        }

        Ok(())
    }

    fn capture_aggregate_symbol_instance(&mut self) -> Result<(), Jbig2Error> {
        let apply_refinement = self
            .stream
            .next_bit()
            .ok_or(Jbig2Error::Truncated(CAPTURED_HUFFMAN_REFINEMENT_BITMAP))?;
        if !apply_refinement {
            return Ok(());
        }

        self.aggregate_refinement_table.decode_value(self.stream)?;
        self.aggregate_refinement_table.decode_value(self.stream)?;
        self.aggregate_refinement_table.decode_value(self.stream)?;
        self.aggregate_refinement_table.decode_value(self.stream)?;
        let bitmap_size = self.aggregate_rsize_table.decode_value(self.stream)?;
        let bitmap_size = i32_to_usize(bitmap_size)
            .map_err(|_| Jbig2Error::InvalidState(HUFFMAN_REFINEMENT_BITMAP_SIZE))?;
        self.stream.align_to_byte_boundary();
        let _ = self
            .stream
            .take_from_byte_len(bitmap_size)
            .ok_or(Jbig2Error::Truncated(CAPTURED_HUFFMAN_REFINEMENT_BITMAP))?;
        Ok(())
    }

    #[cfg(test)]
    fn decode_aggregate_symbol_instance(
        &mut self,
        params: crate::symbol_dictionary::refinement::AggregateRefinementParams<'_>,
        symbol_id: usize,
    ) -> Result<JBig2Image, Jbig2Error> {
        let reference = params.symbols.get(symbol_id)?;
        let apply_refinement = self
            .stream
            .next_bit()
            .ok_or(Jbig2Error::Truncated(HUFFMAN_REFINEMENT_BITMAP))?;
        if !apply_refinement {
            return Ok(reference.clone());
        }

        let delta_width = self.aggregate_refinement_table.decode_value(self.stream)?;
        let delta_height = self.aggregate_refinement_table.decode_value(self.stream)?;
        let delta_x = self.aggregate_refinement_table.decode_value(self.stream)?;
        let delta_y = self.aggregate_refinement_table.decode_value(self.stream)?;
        let bitmap_size = self.aggregate_rsize_table.decode_value(self.stream)?;
        let bitmap_size = i32_to_usize(bitmap_size)
            .map_err(|_| Jbig2Error::InvalidState(HUFFMAN_REFINEMENT_BITMAP_SIZE))?;
        let Some(bitmap_body) = self.take_refinement_bitmap_body(bitmap_size)? else {
            return Ok(reference.clone());
        };
        let mut refinement_reader = BitReader::new(bitmap_body);
        let mut decoder = JBig2ArithDecoder::new(&mut refinement_reader);
        decode_refinement_symbol_from_deltas(
            reference,
            delta_width,
            delta_height,
            delta_x,
            delta_y,
            REFINEMENT_SYMBOL_WIDTH,
            REFINEMENT_SYMBOL_HEIGHT,
            params.refinement,
            aggregate_refinement_reference_offset,
            &mut decoder,
        )
    }

    /// Decode the aggregate-instance count for a refinement-coded symbol.
    fn decode_aggregate_instances(&mut self) -> Result<i32, Jbig2Error> {
        self.agginst_table.decode_value(self.stream)
    }

    /// Consume a declared refinement bitmap body from the current byte position.
    ///
    /// Huffman symbol-dictionary refinement stores the bitmap body as a
    /// byte-aligned payload whose size is declared in the Huffman stream. The
    /// arithmetic refinement decoder must see only that declared slice, so the
    /// remaining Huffman codes stay on the outer stream cursor.
    fn take_refinement_bitmap_body(
        &mut self,
        bitmap_size: usize,
    ) -> Result<Option<&'data [u8]>, Jbig2Error> {
        self.stream.align_to_byte_boundary();
        if bitmap_size == 0 {
            return Ok(None);
        }
        self.stream
            .take_from_byte_len(bitmap_size)
            .map(Some)
            .ok_or(Jbig2Error::Truncated(HUFFMAN_REFINEMENT_BITMAP))
    }

    /// Read `bits` raw symbol-code bits from the stream.
    fn read_raw_bits(&mut self, bits: u8) -> Result<u32, Jbig2Error> {
        let mut value = 0u32;
        for _ in 0..bits {
            let bit = self.stream.next_bit().ok_or(Jbig2Error::Truncated(
                SYMBOL_DICTIONARY_REFINEMENT_SYMBOL_ID,
            ))?;
            value = value
                .checked_shl(1)
                .ok_or(Jbig2Error::Overflow(INTEGER_CONVERSION_OVERFLOW))?;
            if bit {
                value = value
                    .checked_add(1)
                    .ok_or(Jbig2Error::Overflow(INTEGER_CONVERSION_OVERFLOW))?;
            }
        }
        Ok(value)
    }

    /// Decode the collective bitmap for a Huffman width run.
    ///
    /// ITU-T T.88 / ISO/IEC 14492 section 7.4.2 stores either an
    /// uncompressed collective bitmap when `BMSIZE` is zero or an MMR-coded
    /// collective bitmap when `BMSIZE` is positive.
    fn decode_collective_bitmap(
        &mut self,
        total_width: usize,
        height: u16,
    ) -> Result<JBig2Image, Jbig2Error> {
        let bmsize = self.bmsize_table.decode_value(self.stream)?;
        self.stream.align_to_byte_boundary();

        let total_width = usize_to_u16(total_width, COLLECTIVE_BITMAP_WIDTH)?;

        if bmsize == 0 {
            return JBig2Image::decode_uncompressed_collective_bitmap(
                total_width,
                height,
                self.stream,
            );
        }
        let mmr_len =
            i32_to_usize(bmsize).map_err(|_| Jbig2Error::InvalidState(COLLECTIVE_BITMAP_SIZE))?;
        let mmr_data = self
            .stream
            .take_from_byte_len(mmr_len)
            .ok_or(Jbig2Error::Truncated(COLLECTIVE_BITMAP))?;
        let image = decode_mmr_region(total_width, height, mmr_data)?;
        self.stream.align_to_byte_boundary();

        Ok(image)
    }

    /// Decode Huffman-coded symbol export flags.
    ///
    /// ITU-T T.88 / ISO/IEC 14492 section 7.4.2 encodes export flags as
    /// alternating false/true runs. Huffman-coded symbol dictionaries use
    /// standard Huffman table B.1 for these run lengths.
    pub(super) fn decode_export_flags(
        &mut self,
        input_symbol_count: usize,
        new_symbol_count: usize,
    ) -> Result<Vec<bool>, Jbig2Error> {
        let total_symbols = total_symbol_count(input_symbol_count, new_symbol_count)?;
        let mut export_flags = vec![false; total_symbols];
        let mut current_flag = false;
        let mut export_index = 0usize;

        while export_index < total_symbols {
            let run_length = self.export_table.decode_value(self.stream)?;
            let run_length = i32_to_usize(run_length)?;
            let next_index = export_index
                .checked_add(run_length)
                .ok_or(Jbig2Error::Overflow(INTEGER_CONVERSION_OVERFLOW))?;
            let next_index = next_index.min(total_symbols);
            for flag in export_flags.iter_mut().take(next_index).skip(export_index) {
                *flag = current_flag;
            }
            export_index = next_index;
            if export_index >= total_symbols {
                break;
            }
            current_flag = !current_flag;
        }

        Ok(export_flags)
    }
}

/// Decoded Huffman width run metadata from section 7.4.2.
#[derive(Debug, Clone, PartialEq, Eq)]
struct HuffmanWidthRun {
    widths: Vec<u16>,
    total_width: usize,
}

fn aggregate_refinement_reference_offset(size_delta: i32, delta: i32) -> Result<i32, Jbig2Error> {
    (size_delta >> 2)
        .checked_add(delta)
        .ok_or(Jbig2Error::Overflow(INTEGER_CONVERSION_OVERFLOW))
}

#[derive(Debug, Clone)]
enum PendingHuffmanSymbol<'data> {
    Ready(JBig2Image),
    DeferredAggregate(DeferredAggregateRefinedSymbol<'data>),
}

#[cfg(test)]
impl PendingHuffmanSymbol<'_> {
    fn ready_image(&self) -> Option<&JBig2Image> {
        match self {
            Self::Ready(image) => Some(image),
            Self::DeferredAggregate(_) => None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct DeferredAggregateRefinedSymbol<'data> {
    width: u16,
    height: u16,
    body: &'data [u8],
    start_bit_offset: usize,
    symbol_instances: u32,
    symbol_code_length: u8,
    template: RefinementTemplate,
    at: RefinementAdaptiveTemplate,
}

#[derive(Debug)]
struct PendingHuffmanSymbolResolver<'data, 'pending> {
    input_symbols: &'pending [JBig2Image],
    pending_symbols: &'pending [PendingHuffmanSymbol<'data>],
    resolved: Vec<Option<JBig2Image>>,
    resolving: Vec<bool>,
}

#[derive(Debug)]
struct DeferredAggregateSymbolDecoder<'resolver, 'data, 'pending> {
    spec: DeferredAggregateRefinedSymbol<'data>,
    reader: BitReader<'data>,
    resolver: &'resolver mut PendingHuffmanSymbolResolver<'data, 'pending>,
    aggregate_dt_table: StandardHuffmanDecoder,
    aggregate_fs_table: StandardHuffmanDecoder,
    aggregate_ds_table: StandardHuffmanDecoder,
    aggregate_refinement_table: StandardHuffmanDecoder,
    aggregate_rsize_table: StandardHuffmanDecoder,
}

fn ready_symbol_prefix(pending_symbols: &[PendingHuffmanSymbol<'_>]) -> Vec<JBig2Image> {
    pending_symbols
        .iter()
        .filter_map(|symbol| match symbol {
            PendingHuffmanSymbol::Ready(image) => Some(image.clone()),
            PendingHuffmanSymbol::DeferredAggregate(_) => None,
        })
        .collect()
}

impl<'data, 'pending> PendingHuffmanSymbolResolver<'data, 'pending> {
    fn new(
        input_symbols: &'pending [JBig2Image],
        pending_symbols: &'pending [PendingHuffmanSymbol<'data>],
    ) -> Self {
        Self {
            input_symbols,
            pending_symbols,
            resolved: vec![None; pending_symbols.len()],
            resolving: vec![false; pending_symbols.len()],
        }
    }

    fn resolve_all(mut self) -> Result<Vec<JBig2Image>, Jbig2Error> {
        self.pending_symbols
            .iter()
            .enumerate()
            .map(|(index, _)| self.resolve_pending_symbol(index))
            .collect()
    }

    fn resolve_pending_symbol(&mut self, index: usize) -> Result<JBig2Image, Jbig2Error> {
        if let Some(image) = self.resolved.get(index).cloned().flatten() {
            return Ok(image);
        }
        if self.resolving_state(index)? {
            return Err(Jbig2Error::InvalidState(
                DEFERRED_AGGREGATE_SYMBOL_REFERENCE,
            ));
        }
        self.set_resolving_state(index, true)?;

        let symbol = self
            .pending_symbols
            .get(index)
            .cloned()
            .ok_or(Jbig2Error::InvalidState(
                DEFERRED_AGGREGATE_SYMBOL_REFERENCE,
            ))?;
        let image = match symbol {
            PendingHuffmanSymbol::Ready(image) => image,
            PendingHuffmanSymbol::DeferredAggregate(spec) => {
                DeferredAggregateSymbolDecoder::new(self, spec)?.decode()?
            }
        };

        self.set_resolving_state(index, false)?;
        self.store_resolved(index, image.clone())?;
        Ok(image)
    }

    fn resolve_symbol(&mut self, symbol_id: usize) -> Result<JBig2Image, Jbig2Error> {
        if let Some(symbol) = self.input_symbols.get(symbol_id) {
            return Ok(symbol.clone());
        }
        let new_index =
            symbol_id
                .checked_sub(self.input_symbols.len())
                .ok_or(Jbig2Error::InvalidState(
                    AGGREGATE_SYMBOL_DICTIONARY_REFINEMENT_SYMBOL_ID,
                ))?;
        self.resolve_pending_symbol(new_index)
            .map_err(|err| match err {
                Jbig2Error::InvalidState(DEFERRED_AGGREGATE_SYMBOL_REFERENCE) => err,
                _ => Jbig2Error::InvalidState(AGGREGATE_SYMBOL_DICTIONARY_REFINEMENT_SYMBOL_ID),
            })
    }

    fn resolving_state(&self, index: usize) -> Result<bool, Jbig2Error> {
        self.resolving
            .get(index)
            .copied()
            .ok_or(Jbig2Error::InvalidState(
                DEFERRED_AGGREGATE_SYMBOL_REFERENCE,
            ))
    }

    fn set_resolving_state(&mut self, index: usize, value: bool) -> Result<(), Jbig2Error> {
        let state = self
            .resolving
            .get_mut(index)
            .ok_or(Jbig2Error::InvalidState(
                DEFERRED_AGGREGATE_SYMBOL_REFERENCE,
            ))?;
        *state = value;
        Ok(())
    }

    fn store_resolved(&mut self, index: usize, image: JBig2Image) -> Result<(), Jbig2Error> {
        let slot = self
            .resolved
            .get_mut(index)
            .ok_or(Jbig2Error::InvalidState(
                DEFERRED_AGGREGATE_SYMBOL_REFERENCE,
            ))?;
        *slot = Some(image);
        Ok(())
    }
}

impl<'resolver, 'data, 'pending> DeferredAggregateSymbolDecoder<'resolver, 'data, 'pending> {
    fn new(
        resolver: &'resolver mut PendingHuffmanSymbolResolver<'data, 'pending>,
        spec: DeferredAggregateRefinedSymbol<'data>,
    ) -> Result<Self, Jbig2Error> {
        let mut reader = BitReader::new(spec.body);
        reader.set_pos(spec.start_bit_offset);

        Ok(Self {
            spec,
            reader,
            resolver,
            aggregate_dt_table: HuffmanTableSelection::TextRegionDt(0).standard_decoder()?,
            aggregate_fs_table: HuffmanTableSelection::TextRegionFs(0).standard_decoder()?,
            aggregate_ds_table: HuffmanTableSelection::TextRegionDs(0).standard_decoder()?,
            aggregate_refinement_table: text_region_refinement_standard_decoder(0)?,
            aggregate_rsize_table: StandardHuffmanDecoder::new(STANDARD_TABLE_B1)?,
        })
    }

    fn decode(mut self) -> Result<JBig2Image, Jbig2Error> {
        let geometry = aggregate_refinement_geometry();
        let mut image = JBig2Image::try_new(self.spec.width, self.spec.height, None)?;
        let initial_stript = self.aggregate_dt_table.decode_value(&mut self.reader)?;
        let mut state = TextRegionDecodeState::from_initial_delta(initial_stript, 1)?;
        let symbol_instances = self.spec.symbol_instances;

        decode_aggregate_symbol_instances(
            &mut self,
            &mut image,
            geometry,
            &mut state,
            symbol_instances,
            true,
            Self::decode_next_strip_header_delta,
            Self::decode_first_symbol_delta,
            Self::decode_delta_s_or_end,
            Self::decode_symbol_id,
            Self::decode_symbol_instance,
        )?;

        Ok(image)
    }

    fn decode_next_strip_header_delta(&mut self) -> Result<i32, Jbig2Error> {
        self.aggregate_dt_table.decode_value(&mut self.reader)
    }

    fn decode_first_symbol_delta(&mut self) -> Result<i32, Jbig2Error> {
        self.aggregate_fs_table.decode_value(&mut self.reader)
    }

    fn decode_delta_s_or_end(&mut self) -> Result<Option<i32>, Jbig2Error> {
        match self.aggregate_ds_table.decode(&mut self.reader)? {
            HuffmanValue::OutOfBand => Ok(None),
            HuffmanValue::Value(delta_s) => Ok(Some(delta_s)),
        }
    }

    fn decode_symbol_id(&mut self) -> Result<usize, Jbig2Error> {
        read_raw_bits_from_reader(&mut self.reader, self.spec.symbol_code_length)
    }

    fn decode_symbol_instance(&mut self, symbol_id: usize) -> Result<JBig2Image, Jbig2Error> {
        let reference = self.resolver.resolve_symbol(symbol_id)?;
        let apply_refinement = self
            .reader
            .next_bit()
            .ok_or(Jbig2Error::Truncated(HUFFMAN_REFINEMENT_BITMAP))?;
        if !apply_refinement {
            return Ok(reference);
        }

        let delta_width = self
            .aggregate_refinement_table
            .decode_value(&mut self.reader)?;
        let delta_height = self
            .aggregate_refinement_table
            .decode_value(&mut self.reader)?;
        let delta_x = self
            .aggregate_refinement_table
            .decode_value(&mut self.reader)?;
        let delta_y = self
            .aggregate_refinement_table
            .decode_value(&mut self.reader)?;
        let bitmap_size = self.aggregate_rsize_table.decode_value(&mut self.reader)?;
        self.reader.align_to_byte_boundary();
        let bitmap_size = i32_to_usize(bitmap_size)
            .map_err(|_| Jbig2Error::InvalidState(HUFFMAN_REFINEMENT_BITMAP_SIZE))?;
        let bitmap_body = self
            .reader
            .take_from_byte_len(bitmap_size)
            .ok_or(Jbig2Error::Truncated(DEFERRED_HUFFMAN_REFINEMENT_BITMAP))?;
        let mut refinement_reader = BitReader::new(bitmap_body);
        let mut decoder = JBig2ArithDecoder::new(&mut refinement_reader);
        decode_refinement_symbol_from_deltas(
            &reference,
            delta_width,
            delta_height,
            delta_x,
            delta_y,
            REFINEMENT_SYMBOL_WIDTH,
            REFINEMENT_SYMBOL_HEIGHT,
            crate::symbol_dictionary::refinement::SymbolDictionaryRefinementConfig {
                template: self.spec.template,
                at: self.spec.at,
            },
            aggregate_refinement_reference_offset,
            &mut decoder,
        )
    }
}

fn read_raw_bits_from_reader(reader: &mut BitReader<'_>, bits: u8) -> Result<usize, Jbig2Error> {
    let mut value = 0u32;
    for _ in 0..bits {
        let bit = reader.next_bit().ok_or(Jbig2Error::Truncated(
            SYMBOL_DICTIONARY_REFINEMENT_SYMBOL_ID,
        ))?;
        value = value
            .checked_shl(1)
            .ok_or(Jbig2Error::Overflow(INTEGER_CONVERSION_OVERFLOW))?;
        if bit {
            value = value
                .checked_add(1)
                .ok_or(Jbig2Error::Overflow(INTEGER_CONVERSION_OVERFLOW))?;
        }
    }
    usize::try_from(value).map_err(|_| Jbig2Error::Overflow(INTEGER_CONVERSION_OVERFLOW))
}

#[cfg(test)]
mod tests {
    use super::{
        AGGREGATE_SYMBOL_DICTIONARY_REFINEMENT_SYMBOL_ID, HUFFMAN_SYMBOL_HEIGHT, HuffmanDecoder,
        HuffmanSymbolDictionaryDecoder, PendingHuffmanSymbol, RefinementAdaptiveTemplate,
        RefinementTemplate, STANDARD_TABLE_B1, STANDARD_TABLE_B15, StandardHuffmanDecoder,
        text_region_refinement_standard_decoder,
    };
    use crate::{
        error::Jbig2Error,
        huffman::{
            HuffmanValue, STANDARD_TABLE_B2, STANDARD_TABLE_B4,
            test_support::{bits_to_bytes, encode_standard_huffman_value},
        },
        image::JBig2Image,
        segment::{JBig2SegmentResult, ParsedSegment},
        segment_context::SegmentDecodeContext,
        symbol_dictionary::{
            SymbolDictionary, current_symbol_set::CurrentSymbolSet,
            flags::SymbolDictionaryFlagBits, header::ParsedSymbolDictionaryHeader,
            refinement::AggregateRefinementParams,
        },
    };
    use pdf_utils::BitReader;

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

    fn custom_table(seed: i32) -> crate::huffman::CustomHuffmanDecoder {
        let mut data = Vec::new();
        data.push(0);
        data.extend_from_slice(&seed.to_be_bytes());
        data.extend_from_slice(&seed.to_be_bytes());
        let bits = [true, true];
        data.extend_from_slice(&bits_to_bytes(&bits));
        crate::huffman::CustomHuffmanDecoder::parse(&data).expect("custom table")
    }

    fn parsed_header(flags: SymbolDictionaryFlagBits) -> ParsedSymbolDictionaryHeader {
        ParsedSymbolDictionaryHeader {
            flags,
            generic_at: None,
            refinement_at: None,
            num_exported: 1,
            num_new_symbols: 1,
        }
    }

    fn refinement_decoder<'a>(
        stream: &'a mut BitReader<'a>,
        flags: SymbolDictionaryFlagBits,
    ) -> HuffmanSymbolDictionaryDecoder<'a, 'a> {
        HuffmanSymbolDictionaryDecoder {
            stream,
            header: parsed_header(flags),
            dh_table: HuffmanDecoder::Standard(
                StandardHuffmanDecoder::new(STANDARD_TABLE_B4).expect("dh table"),
            ),
            dw_table: HuffmanDecoder::Standard(
                StandardHuffmanDecoder::new(STANDARD_TABLE_B2).expect("dw table"),
            ),
            bmsize_table: HuffmanDecoder::Standard(
                StandardHuffmanDecoder::new(STANDARD_TABLE_B1).expect("bmsize table"),
            ),
            agginst_table: HuffmanDecoder::Standard(
                StandardHuffmanDecoder::new(STANDARD_TABLE_B1).expect("agginst table"),
            ),
            refinement_table: StandardHuffmanDecoder::new(STANDARD_TABLE_B15)
                .expect("refinement table"),
            aggregate_fs_table: crate::huffman::HuffmanTableSelection::TextRegionFs(0)
                .standard_decoder()
                .expect("aggregate fs table"),
            aggregate_ds_table: crate::huffman::HuffmanTableSelection::TextRegionDs(0)
                .standard_decoder()
                .expect("aggregate ds table"),
            aggregate_dt_table: crate::huffman::HuffmanTableSelection::TextRegionDt(0)
                .standard_decoder()
                .expect("aggregate dt table"),
            aggregate_refinement_table: text_region_refinement_standard_decoder(0)
                .expect("aggregate refinement table"),
            aggregate_rsize_table: StandardHuffmanDecoder::new(STANDARD_TABLE_B1)
                .expect("aggregate rsize table"),
            export_table: StandardHuffmanDecoder::new(STANDARD_TABLE_B1).expect("export table"),
        }
    }

    fn append_refined_symbol_payload(
        bits: &mut Vec<bool>,
        b15: &StandardHuffmanDecoder,
        b1: &StandardHuffmanDecoder,
    ) {
        encode_standard_huffman_value(bits, b15, HuffmanValue::Value(0)).expect("rdx");
        encode_standard_huffman_value(bits, b15, HuffmanValue::Value(0)).expect("rdy");
        encode_standard_huffman_value(bits, b1, HuffmanValue::Value(16)).expect("size");
    }

    fn append_width_oob(bits: &mut Vec<bool>) {
        let dw = StandardHuffmanDecoder::new(STANDARD_TABLE_B2).expect("dw");
        encode_standard_huffman_value(bits, &dw, HuffmanValue::OutOfBand).expect("dw oob");
    }

    fn append_dh(bits: &mut Vec<bool>, value: i32) {
        let dh = StandardHuffmanDecoder::new(STANDARD_TABLE_B4).expect("dh");
        encode_standard_huffman_value(bits, &dh, HuffmanValue::Value(value)).expect("dh");
    }

    fn append_dw(bits: &mut Vec<bool>, value: i32) {
        let dw = StandardHuffmanDecoder::new(STANDARD_TABLE_B2).expect("dw");
        encode_standard_huffman_value(bits, &dw, HuffmanValue::Value(value)).expect("dw");
    }

    fn append_agginst(bits: &mut Vec<bool>, value: i32) {
        let b1 = StandardHuffmanDecoder::new(STANDARD_TABLE_B1).expect("b1");
        encode_standard_huffman_value(bits, &b1, HuffmanValue::Value(value)).expect("agginst");
    }

    fn append_symbol_id(bits: &mut Vec<bool>, symbol_id: u32, symbol_code_length: u8) {
        for shift in (0..u32::from(symbol_code_length)).rev() {
            bits.push(((symbol_id >> shift) & 1) != 0);
        }
    }

    fn finish_stream(bits: Vec<bool>, body: &[u8], tail_bits: Vec<bool>) -> Vec<u8> {
        let mut data = bits_to_bytes(&bits);
        data.extend_from_slice(body);
        data.extend_from_slice(&bits_to_bytes(&tail_bits));
        data
    }

    #[test]
    fn constructor_consumes_custom_tables_in_symbol_dictionary_order() -> Result<(), Jbig2Error> {
        let flags = SymbolDictionaryFlagBits::from_bits_retain(
            SymbolDictionaryFlagBits::SDHUFF.bits()
                | SymbolDictionaryFlagBits::SDREFAGG.bits()
                | SymbolDictionaryFlagBits::SDHUFF_DH_MASK.bits()
                | SymbolDictionaryFlagBits::SDHUFF_DW_MASK.bits()
                | SymbolDictionaryFlagBits::SDHUFF_BMSIZE.bits()
                | SymbolDictionaryFlagBits::SDHUFF_AGGINST.bits(),
        );
        let tables = vec![
            custom_table(0),
            custom_table(1),
            custom_table(2),
            custom_table(3),
        ];
        let mut reader = BitReader::new(&[]);
        let decoder =
            HuffmanSymbolDictionaryDecoder::new(&mut reader, parsed_header(flags), tables)?;

        assert!(matches!(decoder.dh_table, HuffmanDecoder::Custom(_)));
        assert!(matches!(decoder.dw_table, HuffmanDecoder::Custom(_)));
        assert!(matches!(decoder.bmsize_table, HuffmanDecoder::Custom(_)));
        assert!(matches!(decoder.agginst_table, HuffmanDecoder::Custom(_)));
        Ok(())
    }

    #[test]
    fn zero_width_refined_symbol_skips_payload_and_keeps_next_width_code_aligned()
    -> Result<(), Jbig2Error> {
        let mut bits = Vec::new();
        append_dh(&mut bits, 1);
        append_dw(&mut bits, 0);
        append_dw(&mut bits, 1);
        append_agginst(&mut bits, 1);
        append_symbol_id(&mut bits, 0, 2);
        let b15 = StandardHuffmanDecoder::new(STANDARD_TABLE_B15)?;
        let b1 = StandardHuffmanDecoder::new(STANDARD_TABLE_B1)?;
        append_refined_symbol_payload(&mut bits, &b15, &b1);

        let mut tail_bits = Vec::new();
        append_width_oob(&mut tail_bits);
        let data = finish_stream(bits, &[0xff; 16], tail_bits);
        let mut reader = BitReader::new(&data);
        let mut decoder = refinement_decoder(
            &mut reader,
            SymbolDictionaryFlagBits::SDHUFF
                | SymbolDictionaryFlagBits::SDREFAGG
                | SymbolDictionaryFlagBits::SDHUFF_AGGINST,
        );
        let input_symbols = [JBig2Image::new(1, 1)];
        let symbols = decoder.decode_new_symbols(&input_symbols, 2)?;

        assert_eq!(symbols.first(), Some(&JBig2Image::empty()));
        let second = symbols.get(1).ok_or(Jbig2Error::MissingSymbol("decoded"))?;
        assert_eq!(second.width(), 1);
        assert_eq!(second.height(), 1);
        Ok(())
    }

    #[test]
    fn zero_height_refined_symbol_skips_payload_and_keeps_next_width_code_aligned()
    -> Result<(), Jbig2Error> {
        let mut bits = Vec::new();
        append_dw(&mut bits, 1);
        append_width_oob(&mut bits);
        append_dw(&mut bits, 1);
        append_agginst(&mut bits, 1);
        append_symbol_id(&mut bits, 0, 2);
        let b15 = StandardHuffmanDecoder::new(STANDARD_TABLE_B15)?;
        let b1 = StandardHuffmanDecoder::new(STANDARD_TABLE_B1)?;
        append_refined_symbol_payload(&mut bits, &b15, &b1);

        let mut tail_bits = Vec::new();
        append_width_oob(&mut tail_bits);
        let data = finish_stream(bits, &[0xff; 16], tail_bits);
        let mut reader = BitReader::new(&data);
        let mut decoder = refinement_decoder(
            &mut reader,
            SymbolDictionaryFlagBits::SDHUFF
                | SymbolDictionaryFlagBits::SDREFAGG
                | SymbolDictionaryFlagBits::SDHUFF_AGGINST,
        );
        let input_symbols = [JBig2Image::new(1, 1)];
        let mut symbols = Vec::new();
        decoder.decode_refined_width_run(&input_symbols, &mut symbols, 2, 0, 2)?;
        decoder.decode_refined_width_run(&input_symbols, &mut symbols, 2, 1, 2)?;

        assert_eq!(
            symbols.first().and_then(PendingHuffmanSymbol::ready_image),
            Some(&JBig2Image::empty())
        );
        let second = symbols
            .get(1)
            .and_then(PendingHuffmanSymbol::ready_image)
            .ok_or(Jbig2Error::MissingSymbol("decoded"))?;
        assert_eq!(second.width(), 1);
        assert_eq!(second.height(), 1);
        Ok(())
    }

    #[test]
    fn refined_single_symbol_consumes_only_declared_body_before_next_width_code()
    -> Result<(), Jbig2Error> {
        let mut bits = Vec::new();
        append_agginst(&mut bits, 1);
        append_symbol_id(&mut bits, 0, 1);
        let b15 = StandardHuffmanDecoder::new(STANDARD_TABLE_B15)?;
        let b1 = StandardHuffmanDecoder::new(STANDARD_TABLE_B1)?;
        append_refined_symbol_payload(&mut bits, &b15, &b1);

        let mut tail_bits = Vec::new();
        append_width_oob(&mut tail_bits);
        let data = finish_stream(bits, &[0xff; 16], tail_bits);
        let mut reader = BitReader::new(&data);
        let mut decoder = refinement_decoder(
            &mut reader,
            SymbolDictionaryFlagBits::SDHUFF
                | SymbolDictionaryFlagBits::SDREFAGG
                | SymbolDictionaryFlagBits::SDHUFF_AGGINST,
        );
        let input_symbols = [JBig2Image::new(1, 1)];
        let image = decoder.decode_refined_symbol(&input_symbols, &[], 1, 1, 1)?;
        let image = image
            .ready_image()
            .ok_or(Jbig2Error::MissingSymbol("decoded"))?;

        assert_eq!(image.width(), 1);
        assert_eq!(image.height(), 1);
        assert_eq!(
            decoder.dw_table.decode(decoder.stream)?,
            HuffmanValue::OutOfBand
        );
        Ok(())
    }

    #[test]
    fn refined_single_symbol_uses_declared_body_even_before_stream_end() -> Result<(), Jbig2Error> {
        let mut bits = Vec::new();
        append_agginst(&mut bits, 1);
        append_symbol_id(&mut bits, 0, 1);
        let b15 = StandardHuffmanDecoder::new(STANDARD_TABLE_B15)?;
        let b1 = StandardHuffmanDecoder::new(STANDARD_TABLE_B1)?;
        append_refined_symbol_payload(&mut bits, &b15, &b1);

        let data = finish_stream(bits, &[0xff; 16], Vec::new());
        let mut reader = BitReader::new(&data);
        let mut decoder = refinement_decoder(
            &mut reader,
            SymbolDictionaryFlagBits::SDHUFF
                | SymbolDictionaryFlagBits::SDREFAGG
                | SymbolDictionaryFlagBits::SDHUFF_AGGINST,
        );
        let input_symbols = [JBig2Image::new(1, 1)];
        let image = decoder.decode_refined_symbol(&input_symbols, &[], 1, 1, 1)?;
        let image = image
            .ready_image()
            .ok_or(Jbig2Error::MissingSymbol("decoded"))?;

        assert_eq!(image.width(), 1);
        assert_eq!(image.height(), 1);
        Ok(())
    }

    #[test]
    fn refined_aggregate_symbol_uses_declared_body_even_before_stream_end() -> Result<(), Jbig2Error>
    {
        let mut bits = Vec::new();
        append_agginst(&mut bits, 2);
        append_symbol_id(&mut bits, 0, 1);
        let b15 = StandardHuffmanDecoder::new(STANDARD_TABLE_B15)?;
        let b1 = StandardHuffmanDecoder::new(STANDARD_TABLE_B1)?;
        append_refined_symbol_payload(&mut bits, &b15, &b1);

        let data = finish_stream(bits, &[0xff; 16], Vec::new());
        let mut reader = BitReader::new(&data);
        let mut decoder = refinement_decoder(
            &mut reader,
            SymbolDictionaryFlagBits::SDHUFF
                | SymbolDictionaryFlagBits::SDREFAGG
                | SymbolDictionaryFlagBits::SDHUFF_AGGINST,
        );
        let input_symbols = [JBig2Image::new(1, 1)];
        let params = AggregateRefinementParams {
            symbols: CurrentSymbolSet::new(
                &input_symbols,
                &[],
                AGGREGATE_SYMBOL_DICTIONARY_REFINEMENT_SYMBOL_ID,
            ),
            symbol_code_length: 1,
            refinement: crate::symbol_dictionary::refinement::SymbolDictionaryRefinementConfig {
                template: RefinementTemplate::Template1,
                at: RefinementAdaptiveTemplate::default_for(RefinementTemplate::Template1),
            },
        };
        let image = decoder.decode_aggregate_symbol_instance(params, 0)?;

        assert_eq!(image.width(), 1);
        assert_eq!(image.height(), 1);
        Ok(())
    }

    #[test]
    fn refined_decode_treats_zero_aggregate_instances_as_single_symbol() -> Result<(), Jbig2Error> {
        let mut bits = Vec::new();
        append_agginst(&mut bits, 0);
        append_symbol_id(&mut bits, 0, 1);
        let b15 = StandardHuffmanDecoder::new(STANDARD_TABLE_B15)?;
        let b1 = StandardHuffmanDecoder::new(STANDARD_TABLE_B1)?;
        append_refined_symbol_payload(&mut bits, &b15, &b1);
        let data = finish_stream(bits, &[0xff; 16], Vec::new());
        let mut reader = BitReader::new(&data);
        let mut decoder = refinement_decoder(
            &mut reader,
            SymbolDictionaryFlagBits::SDHUFF
                | SymbolDictionaryFlagBits::SDREFAGG
                | SymbolDictionaryFlagBits::SDHUFF_AGGINST,
        );
        let input_symbols = [JBig2Image::new(1, 1)];

        let image = decoder.decode_refined_symbol(&input_symbols, &[], 1, 1, 1)?;
        let image = image
            .ready_image()
            .ok_or(Jbig2Error::MissingSymbol("decoded"))?;
        assert_eq!(image.width(), 1);
        assert_eq!(image.height(), 1);
        Ok(())
    }

    #[test]
    fn aggregate_refined_symbol_consumes_only_declared_body_before_next_width_code()
    -> Result<(), Jbig2Error> {
        let mut bits = Vec::new();
        let b15 = StandardHuffmanDecoder::new(STANDARD_TABLE_B15)?;
        let b1 = StandardHuffmanDecoder::new(STANDARD_TABLE_B1)?;
        bits.push(true);
        encode_standard_huffman_value(&mut bits, &b15, HuffmanValue::Value(0)).expect("rdw");
        encode_standard_huffman_value(&mut bits, &b15, HuffmanValue::Value(0)).expect("rdh");
        encode_standard_huffman_value(&mut bits, &b15, HuffmanValue::Value(0)).expect("rdx");
        encode_standard_huffman_value(&mut bits, &b15, HuffmanValue::Value(0)).expect("rdy");
        encode_standard_huffman_value(&mut bits, &b1, HuffmanValue::Value(16)).expect("size");

        let mut tail_bits = Vec::new();
        append_width_oob(&mut tail_bits);
        let data = finish_stream(bits, &[0xff; 16], tail_bits);
        let mut reader = BitReader::new(&data);
        let mut decoder = refinement_decoder(
            &mut reader,
            SymbolDictionaryFlagBits::SDHUFF
                | SymbolDictionaryFlagBits::SDREFAGG
                | SymbolDictionaryFlagBits::SDHUFF_AGGINST,
        );
        let input_symbols = [JBig2Image::new(1, 1)];
        let params = AggregateRefinementParams {
            symbols: CurrentSymbolSet::new(
                &input_symbols,
                &[],
                AGGREGATE_SYMBOL_DICTIONARY_REFINEMENT_SYMBOL_ID,
            ),
            symbol_code_length: 1,
            refinement: crate::symbol_dictionary::refinement::SymbolDictionaryRefinementConfig {
                template: RefinementTemplate::Template0,
                at: RefinementAdaptiveTemplate::default_for(RefinementTemplate::Template0),
            },
        };
        let image = decoder.decode_aggregate_symbol_instance(params, 0)?;

        assert_eq!(image.width(), 1);
        assert_eq!(image.height(), 1);
        assert_eq!(
            decoder.dw_table.decode(decoder.stream)?,
            HuffmanValue::OutOfBand
        );
        Ok(())
    }

    #[test]
    fn sample_second_dictionary_exports_expected_symbol_count() -> Result<(), Jbig2Error> {
        let data = textrefine_jbig2_stream();
        let mut reader = BitReader::new(data);
        let mut decoded_segments = Vec::new();

        let page_segment = ParsedSegment::try_from(&mut reader).expect("page segment");
        let page_end = reader.byte_pos() + page_segment.data_length.expect("page length");
        reader.set_byte_pos_preserving_offset(page_end);
        decoded_segments.push(page_segment);

        let mut first_dict_segment =
            ParsedSegment::try_from(&mut reader).expect("first dictionary");
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
            first_dict_segment.result =
                JBig2SegmentResult::SymbolDictionary(SymbolDictionary::from_reader(&mut context)?);
        }
        reader.set_byte_pos_preserving_offset(first_dict_end);
        decoded_segments.push(first_dict_segment);

        let second_dict_segment = ParsedSegment::try_from(&mut reader).expect("second dictionary");
        let second_dict_end = reader.byte_pos()
            + second_dict_segment
                .data_length
                .expect("second dictionary length");
        let input_symbols = {
            let context = SegmentDecodeContext::new(
                &second_dict_segment,
                &mut reader,
                second_dict_end,
                &decoded_segments,
                &[],
            );
            context.referred_symbol_images()?
        };
        let header = ParsedSymbolDictionaryHeader::try_from(&mut reader)?;
        let mut decoder = HuffmanSymbolDictionaryDecoder::new(&mut reader, header, Vec::new())?;
        let new_symbols = decoder.decode_new_symbols(&input_symbols, header.num_new_symbols)?;
        let export_flags = decoder.decode_export_flags(input_symbols.len(), new_symbols.len())?;

        assert_eq!(header.num_exported, 11);
        assert_eq!(header.num_new_symbols, 4);
        assert_eq!(input_symbols.len(), 7);
        assert_eq!(new_symbols.len(), 4);
        assert_eq!(
            export_flags.iter().copied().filter(|flag| *flag).count(),
            header.num_exported,
        );
        Ok(())
    }

    #[test]
    fn sample_second_dictionary_uses_aggregate_refinement_symbols() -> Result<(), Jbig2Error> {
        let data = textrefine_jbig2_stream();
        let mut reader = BitReader::new(data);
        let mut decoded_segments = Vec::new();

        let page_segment = ParsedSegment::try_from(&mut reader).expect("page segment");
        let page_end = reader.byte_pos() + page_segment.data_length.expect("page length");
        reader.set_byte_pos_preserving_offset(page_end);
        decoded_segments.push(page_segment);

        let mut first_dict_segment =
            ParsedSegment::try_from(&mut reader).expect("first dictionary");
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
            first_dict_segment.result =
                JBig2SegmentResult::SymbolDictionary(SymbolDictionary::from_reader(&mut context)?);
        }
        reader.set_byte_pos_preserving_offset(first_dict_end);
        decoded_segments.push(first_dict_segment);

        let second_dict_segment = ParsedSegment::try_from(&mut reader).expect("second dictionary");
        let second_dict_end = reader.byte_pos()
            + second_dict_segment
                .data_length
                .expect("second dictionary length");
        let input_symbols = {
            let context = SegmentDecodeContext::new(
                &second_dict_segment,
                &mut reader,
                second_dict_end,
                &decoded_segments,
                &[],
            );
            context.referred_symbol_images()?
        };
        let header = ParsedSymbolDictionaryHeader::try_from(&mut reader)?;
        let mut decoder = HuffmanSymbolDictionaryDecoder::new(&mut reader, header, Vec::new())?;
        let symbol_code_length =
            crate::util::ceil_log2(input_symbols.len().saturating_add(header.num_new_symbols))?
                .max(1);
        let mut pending_symbols = Vec::with_capacity(header.num_new_symbols);
        let mut height = 0i32;

        while pending_symbols.len() < header.num_new_symbols {
            let delta_height = decoder.dh_table.decode_value(decoder.stream)?;
            height += delta_height;
            let height = u16::try_from(height)
                .map_err(|_| Jbig2Error::InvalidState(HUFFMAN_SYMBOL_HEIGHT))?;
            decoder.decode_refined_width_run(
                &input_symbols,
                &mut pending_symbols,
                header.num_new_symbols,
                height,
                symbol_code_length,
            )?;
        }

        assert_eq!(pending_symbols.len(), 4);
        assert!(
            pending_symbols
                .iter()
                .any(|symbol| matches!(symbol, PendingHuffmanSymbol::DeferredAggregate(_)))
        );
        Ok(())
    }
}
