use crate::{
    arith_decoder::{JBig2ArithDecoder, JBig2ArithIntegerContext},
    compose_op::ComposeOp,
    error::Jbig2Error,
    generic_refinement_region::{
        GenericRefinementRegionDecode, RefinementAdaptiveTemplate, RefinementTemplate,
    },
    generic_region::{GenericRegion, GenericRegionTemplate},
    image::JBig2Image,
    symbol_dictionary::{
        export::{fill_export_flag_run, total_symbol_count},
        flags::SymbolDictionaryFlagBits,
        header::ParsedSymbolDictionaryHeader,
    },
    text_region::{
        geometry::{TextRegionGeometry, TextRegionRefCorner},
        state::{TextRegionDecodeState, advance_curs_by_delta_s},
    },
    util::{
        INTEGER_CONVERSION_OVERFLOW, ceil_log2, i32_to_usize, refined_dimension,
        refinement_reference_offset,
    },
};

const ARITHMETIC_SYMBOL_HEIGHT: &str = "arithmetic symbol height";
const ARITHMETIC_SYMBOL_WIDTH: &str = "arithmetic symbol width";
const AGGREGATE_SYMBOL_DELTA_T: &str = "aggregate symbol delta t";
const AGGREGATE_SYMBOL_FIRST_S_DELTA: &str = "aggregate symbol first-s delta";
const REFINEMENT_AGGREGATE_INSTANCES: &str = "refinement aggregate instances";
const REFINEMENT_SYMBOL_ID: &str = "refinement symbol id";
const REFINEMENT_SYMBOL_WIDTH: &str = "refinement symbol width";
const REFINEMENT_SYMBOL_HEIGHT: &str = "refinement symbol height";
const REFINEMENT_SYMBOL_X: &str = "refinement symbol x";
const REFINEMENT_SYMBOL_Y: &str = "refinement symbol y";
const SYMBOL_DICTIONARY_AT_DATA: &str = "symbol dictionary AT data";
const SYMBOL_DICTIONARY_HEIGHT_DELTA: &str = "symbol dictionary height delta";
const SYMBOL_DICTIONARY_WIDTH_RUN: &str = "symbol dictionary width run";
const SYMBOL_EXPORT_RUN_LENGTH: &str = "symbol export run length";

/// Decode arithmetic-coded symbols declared by a symbol dictionary header.
///
/// ITU-T T.88 / ISO/IEC 14492 section 7.4.2 uses arithmetic integer contexts
/// for symbol height deltas and width deltas, then decodes each symbol bitmap
/// as a generic region using the dictionary's `SDTEMPLATE`.
pub(super) fn decode_arithmetic_symbol_dictionary(
    header: &ParsedSymbolDictionaryHeader,
    input_symbols: &[JBig2Image],
    decoder: &mut JBig2ArithDecoder<'_, '_>,
) -> Result<Vec<JBig2Image>, Jbig2Error> {
    let mut new_symbols = Vec::with_capacity(header.num_new_symbols);
    let symbol_code_length = ceil_log2(input_symbols.len().saturating_add(header.num_new_symbols))?;
    let mut height = 0i32;

    while new_symbols.len() < header.num_new_symbols {
        let delta_height = decoder.decode_required_integer(
            JBig2ArithIntegerContext::SymbolHeightDelta,
            SYMBOL_DICTIONARY_HEIGHT_DELTA,
        )?;
        height = height
            .checked_add(delta_height)
            .ok_or(Jbig2Error::Overflow(INTEGER_CONVERSION_OVERFLOW))?;
        let height = u16::try_from(height)
            .map_err(|_| Jbig2Error::InvalidState(ARITHMETIC_SYMBOL_HEIGHT))?;
        decode_arithmetic_symbol_width_run(
            header,
            input_symbols,
            symbol_code_length,
            height,
            decoder,
            &mut new_symbols,
        )?;
    }

    Ok(new_symbols)
}

/// Decode one arithmetic symbol width run for a shared symbol height.
///
/// ITU-T T.88 / ISO/IEC 14492 section 7.4.2 terminates a width run with an
/// out-of-band result from the symbol width delta arithmetic integer context.
fn decode_arithmetic_symbol_width_run(
    header: &ParsedSymbolDictionaryHeader,
    input_symbols: &[JBig2Image],
    symbol_code_length: u8,
    height: u16,
    decoder: &mut JBig2ArithDecoder<'_, '_>,
    new_symbols: &mut Vec<JBig2Image>,
) -> Result<(), Jbig2Error> {
    let generic_at = header
        .generic_at
        .ok_or(Jbig2Error::InvalidState(SYMBOL_DICTIONARY_AT_DATA))?;
    let mut width = 0i32;

    loop {
        let Some(delta_width) =
            decoder.decode_integer(JBig2ArithIntegerContext::SymbolWidthDelta)?
        else {
            break;
        };
        width = width
            .checked_add(delta_width)
            .ok_or(Jbig2Error::Overflow(INTEGER_CONVERSION_OVERFLOW))?;
        let width =
            u16::try_from(width).map_err(|_| Jbig2Error::InvalidState(ARITHMETIC_SYMBOL_WIDTH))?;
        let image = if header.flags.contains(SymbolDictionaryFlagBits::SDREFAGG) {
            decode_refined_symbol(
                header,
                input_symbols,
                new_symbols,
                symbol_code_length,
                width,
                height,
                decoder,
            )?
        } else {
            GenericRegion::new_arithmetic(
                width,
                height,
                GenericRegionTemplate::try_from(header.flags.sdtemplate())?,
                false,
                generic_at,
            )?
            .decode_arithmetic_with_decoder(decoder, None)?
        };
        new_symbols.push(image);
        if new_symbols.len() > header.num_new_symbols {
            return Err(Jbig2Error::InvalidState(SYMBOL_DICTIONARY_WIDTH_RUN));
        }
    }

    Ok(())
}

/// Decode one single-instance refinement-coded symbol dictionary entry.
fn decode_refined_symbol(
    header: &ParsedSymbolDictionaryHeader,
    input_symbols: &[JBig2Image],
    new_symbols: &[JBig2Image],
    symbol_code_length: u8,
    width: u16,
    height: u16,
    decoder: &mut JBig2ArithDecoder<'_, '_>,
) -> Result<JBig2Image, Jbig2Error> {
    let aggregate_instances = decoder.decode_required_integer(
        JBig2ArithIntegerContext::RefinementAggregateInstances,
        REFINEMENT_AGGREGATE_INSTANCES,
    )?;
    if aggregate_instances > 1 {
        let symbols = CurrentSymbolSet {
            input_symbols,
            new_symbols,
        };
        let template = RefinementTemplate::from_flag(
            header
                .flags
                .contains(SymbolDictionaryFlagBits::SDR_TEMPLATE),
        );
        let at = header
            .refinement_at
            .unwrap_or_else(|| RefinementAdaptiveTemplate::default_for(template));
        return decode_aggregate_refined_symbol(
            width,
            height,
            aggregate_instances,
            AggregateRefinementParams {
                symbols,
                symbol_code_length,
                template,
                at,
            },
            decoder,
        );
    }
    if aggregate_instances != 1 {
        return Err(Jbig2Error::InvalidState(REFINEMENT_AGGREGATE_INSTANCES));
    }

    let symbol_id = usize::try_from(decoder.decode_iaid(symbol_code_length)?)
        .map_err(|_| Jbig2Error::Overflow(INTEGER_CONVERSION_OVERFLOW))?;
    let reference = referenced_symbol(input_symbols, new_symbols, symbol_id)?;
    let delta_x = decoder.decode_required_integer(
        JBig2ArithIntegerContext::RefinementDeltaX,
        REFINEMENT_SYMBOL_X,
    )?;
    let delta_y = decoder.decode_required_integer(
        JBig2ArithIntegerContext::RefinementDeltaY,
        REFINEMENT_SYMBOL_Y,
    )?;
    let template = RefinementTemplate::from_flag(
        header
            .flags
            .contains(SymbolDictionaryFlagBits::SDR_TEMPLATE),
    );
    let reference_dx = delta_x;
    let reference_dy = delta_y;

    GenericRefinementRegionDecode::new(
        width,
        height,
        template,
        false,
        header
            .refinement_at
            .unwrap_or_else(|| RefinementAdaptiveTemplate::default_for(template)),
        reference_dx,
        reference_dy,
    )
    .decode(reference, decoder)
}

fn decode_aggregate_refined_symbol(
    width: u16,
    height: u16,
    aggregate_instances: i32,
    params: AggregateRefinementParams<'_>,
    decoder: &mut JBig2ArithDecoder<'_, '_>,
) -> Result<JBig2Image, Jbig2Error> {
    let symbol_instances = u32::try_from(aggregate_instances)
        .map_err(|_| Jbig2Error::InvalidState(REFINEMENT_AGGREGATE_INSTANCES))?;
    let geometry = TextRegionGeometry::new(false, TextRegionRefCorner::TopLeft);
    let mut image = JBig2Image::try_new(width, height, None)?;
    let initial_stript = decoder.decode_required_integer(
        JBig2ArithIntegerContext::TextDeltaT,
        AGGREGATE_SYMBOL_DELTA_T,
    )?;
    let mut state = TextRegionDecodeState::from_initial_delta(initial_stript, 1)?;

    while !state.is_complete(symbol_instances) {
        decode_aggregate_symbol_strip(
            &mut image,
            params,
            geometry,
            decoder,
            &mut state,
            symbol_instances,
        )?;
    }

    Ok(image)
}

fn decode_aggregate_symbol_strip(
    image: &mut JBig2Image,
    params: AggregateRefinementParams<'_>,
    geometry: TextRegionGeometry,
    decoder: &mut JBig2ArithDecoder<'_, '_>,
    state: &mut TextRegionDecodeState,
    symbol_instances: u32,
) -> Result<(), Jbig2Error> {
    let delta_t = decoder.decode_required_integer(
        JBig2ArithIntegerContext::TextDeltaT,
        AGGREGATE_SYMBOL_DELTA_T,
    )?;
    state.advance_strip(delta_t, 1)?;
    let delta_first_s = decoder.decode_required_integer(
        JBig2ArithIntegerContext::TextFirstS,
        AGGREGATE_SYMBOL_FIRST_S_DELTA,
    )?;
    let mut curs = state.consume_first_s_delta(delta_first_s)?;

    loop {
        let symbol_id = usize::try_from(decoder.decode_iaid(params.symbol_code_length)?)
            .map_err(|_| Jbig2Error::Overflow(INTEGER_CONVERSION_OVERFLOW))?;
        let symbol = decode_aggregate_symbol_instance(params, symbol_id, decoder)?;
        let placed_curs =
            geometry.adjust_curs_before_placement(curs, symbol.width(), symbol.height())?;
        let placement =
            geometry.placement_for(placed_curs, state.stript(), symbol.width(), symbol.height())?;
        symbol.compose_clipped_to(image, placement.x, placement.y, ComposeOp::Or);
        curs =
            geometry.advance_curs_after_placement(placed_curs, symbol.width(), symbol.height())?;
        state.record_instance()?;

        let Some(delta_s) = decoder.decode_integer(JBig2ArithIntegerContext::TextDeltaS)? else {
            break;
        };
        curs = advance_curs_by_delta_s(curs, delta_s, 0)?;
        if state.is_complete(symbol_instances) {
            break;
        }
    }

    Ok(())
}

fn decode_aggregate_symbol_instance(
    params: AggregateRefinementParams<'_>,
    symbol_id: usize,
    decoder: &mut JBig2ArithDecoder<'_, '_>,
) -> Result<JBig2Image, Jbig2Error> {
    let reference = params.symbols.get(symbol_id)?;
    let apply_refinement = decoder.decode_required_integer(
        JBig2ArithIntegerContext::RefinementInstance,
        REFINEMENT_AGGREGATE_INSTANCES,
    )? != 0;
    if !apply_refinement {
        return Ok(reference.clone());
    }

    let delta_width = decoder.decode_required_integer(
        JBig2ArithIntegerContext::RefinementDeltaWidth,
        REFINEMENT_SYMBOL_WIDTH,
    )?;
    let delta_height = decoder.decode_required_integer(
        JBig2ArithIntegerContext::RefinementDeltaHeight,
        REFINEMENT_SYMBOL_HEIGHT,
    )?;
    let delta_x = decoder.decode_required_integer(
        JBig2ArithIntegerContext::RefinementDeltaX,
        REFINEMENT_SYMBOL_X,
    )?;
    let delta_y = decoder.decode_required_integer(
        JBig2ArithIntegerContext::RefinementDeltaY,
        REFINEMENT_SYMBOL_Y,
    )?;
    let width = refined_dimension(reference.width(), delta_width, REFINEMENT_SYMBOL_WIDTH)?;
    let height = refined_dimension(reference.height(), delta_height, REFINEMENT_SYMBOL_HEIGHT)?;
    let reference_dx = refinement_reference_offset(delta_width, delta_x)?;
    let reference_dy = refinement_reference_offset(delta_height, delta_y)?;
    GenericRefinementRegionDecode::new(
        width,
        height,
        params.template,
        false,
        params.at,
        reference_dx,
        reference_dy,
    )
    .decode(reference, decoder)
}

fn referenced_symbol<'a>(
    input_symbols: &'a [JBig2Image],
    new_symbols: &'a [JBig2Image],
    symbol_id: usize,
) -> Result<&'a JBig2Image, Jbig2Error> {
    if let Some(symbol) = input_symbols.get(symbol_id) {
        return Ok(symbol);
    }
    let new_index = symbol_id
        .checked_sub(input_symbols.len())
        .ok_or(Jbig2Error::InvalidState(REFINEMENT_SYMBOL_ID))?;
    new_symbols
        .get(new_index)
        .ok_or(Jbig2Error::InvalidState(REFINEMENT_SYMBOL_ID))
}

#[derive(Debug, Clone, Copy)]
struct CurrentSymbolSet<'a> {
    input_symbols: &'a [JBig2Image],
    new_symbols: &'a [JBig2Image],
}

#[derive(Debug, Clone, Copy)]
struct AggregateRefinementParams<'a> {
    symbols: CurrentSymbolSet<'a>,
    symbol_code_length: u8,
    template: RefinementTemplate,
    at: RefinementAdaptiveTemplate,
}

impl<'a> CurrentSymbolSet<'a> {
    fn get(self, symbol_id: usize) -> Result<&'a JBig2Image, Jbig2Error> {
        referenced_symbol(self.input_symbols, self.new_symbols, symbol_id)
    }
}

/// Decode arithmetic-coded symbol export flags.
///
/// ITU-T T.88 / ISO/IEC 14492 section 7.4.2 encodes export flags as
/// alternating false/true run lengths using the `IAEX` arithmetic integer
/// context.
pub(super) fn decode_export_flags(
    decoder: &mut JBig2ArithDecoder<'_, '_>,
    input_symbol_count: usize,
    new_symbol_count: usize,
) -> Result<Vec<bool>, Jbig2Error> {
    let total_symbols = total_symbol_count(input_symbol_count, new_symbol_count)?;
    let mut export_flags = vec![false; total_symbols];
    let mut current_flag = false;
    let mut export_index = 0usize;

    while export_index < total_symbols {
        let run_length = decoder.decode_required_integer(
            JBig2ArithIntegerContext::SymbolExportRunLength,
            SYMBOL_EXPORT_RUN_LENGTH,
        )?;
        let run_length = i32_to_usize(run_length)?;
        export_index =
            fill_export_flag_run(&mut export_flags, export_index, run_length, current_flag)?;
        current_flag = !current_flag;
    }

    Ok(export_flags)
}
