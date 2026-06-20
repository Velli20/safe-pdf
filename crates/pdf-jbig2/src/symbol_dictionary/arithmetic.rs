use crate::{
    arith_decoder::{JBig2ArithDecoder, JBig2ArithIntegerContext},
    error::Jbig2Error,
    generic_region::{GenericRegion, GenericRegionTemplate},
    image::JBig2Image,
    symbol_dictionary::{
        aggregate::decode_aggregate_symbol_instances,
        export::{fill_export_flag_run, total_symbol_count},
        flags::SymbolDictionaryFlagBits,
        header::ParsedSymbolDictionaryHeader,
        refinement::{
            AggregateRefinementParams, aggregate_refinement_geometry, aggregate_refinement_params,
            decode_refinement_symbol_from_deltas,
        },
    },
    text_region::state::TextRegionDecodeState,
    util::{INTEGER_CONVERSION_OVERFLOW, ceil_log2, i32_to_usize},
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
    let params = aggregate_refinement_params(
        input_symbols,
        new_symbols,
        REFINEMENT_SYMBOL_ID,
        symbol_code_length,
        header,
    );
    if aggregate_instances > 1 {
        let symbol_instances = u32::try_from(aggregate_instances)
            .map_err(|_| Jbig2Error::InvalidState(REFINEMENT_AGGREGATE_INSTANCES))?;
        let mut image = JBig2Image::try_new(width, height, None)?;
        let mut state = TextRegionDecodeState::from_initial_delta(
            decoder.decode_required_integer(
                JBig2ArithIntegerContext::TextDeltaT,
                AGGREGATE_SYMBOL_DELTA_T,
            )?,
            1,
        )?;
        let geometry = aggregate_refinement_geometry();

        decode_aggregate_symbol_instances(
            decoder,
            &mut image,
            geometry,
            &mut state,
            symbol_instances,
            false,
            |decoder| {
                decoder.decode_required_integer(
                    JBig2ArithIntegerContext::TextDeltaT,
                    AGGREGATE_SYMBOL_DELTA_T,
                )
            },
            |decoder| {
                decoder.decode_required_integer(
                    JBig2ArithIntegerContext::TextFirstS,
                    AGGREGATE_SYMBOL_FIRST_S_DELTA,
                )
            },
            |decoder| decoder.decode_integer(JBig2ArithIntegerContext::TextDeltaS),
            |decoder| {
                usize::try_from(decoder.decode_iaid(params.symbol_code_length)?)
                    .map_err(|_| Jbig2Error::Overflow(INTEGER_CONVERSION_OVERFLOW))
            },
            |decoder, symbol_id| decode_aggregate_symbol_instance(params, symbol_id, decoder),
        )?;

        return Ok(image);
    }
    if aggregate_instances < 0 {
        return Err(Jbig2Error::InvalidState(REFINEMENT_AGGREGATE_INSTANCES));
    }

    let symbol_id = usize::try_from(decoder.decode_iaid(symbol_code_length)?)
        .map_err(|_| Jbig2Error::Overflow(INTEGER_CONVERSION_OVERFLOW))?;
    let reference = params.symbols.get(symbol_id)?;
    let delta_x = decoder.decode_required_integer(
        JBig2ArithIntegerContext::RefinementDeltaX,
        REFINEMENT_SYMBOL_X,
    )?;
    let delta_y = decoder.decode_required_integer(
        JBig2ArithIntegerContext::RefinementDeltaY,
        REFINEMENT_SYMBOL_Y,
    )?;
    decode_refinement_symbol_from_deltas(
        reference,
        delta_x,
        delta_y,
        delta_x,
        delta_y,
        REFINEMENT_SYMBOL_WIDTH,
        REFINEMENT_SYMBOL_HEIGHT,
        params.refinement,
        |_, delta| Ok(delta),
        decoder,
    )
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
    decode_refinement_symbol_from_deltas(
        reference,
        delta_width,
        delta_height,
        delta_x,
        delta_y,
        REFINEMENT_SYMBOL_WIDTH,
        REFINEMENT_SYMBOL_HEIGHT,
        params.refinement,
        |_, delta| Ok(delta),
        decoder,
    )
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
