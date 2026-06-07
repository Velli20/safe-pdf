use crate::{
    arith_decoder::{JBig2ArithDecoder, JBig2ArithIntegerContext},
    error::Jbig2Error,
    generic_region::{GenericRegion, GenericRegionTemplate},
    image::JBig2Image,
    symbol_dictionary::{
        export::{fill_export_flag_run, total_symbol_count},
        header::ParsedSymbolDictionaryHeader,
    },
    util::i32_to_usize,
};

const ARITHMETIC_SYMBOL_HEIGHT: &str = "arithmetic symbol height";
const ARITHMETIC_SYMBOL_WIDTH: &str = "arithmetic symbol width";
const INTEGER_CONVERSION_OVERFLOW: &str = "integer conversion overflow";
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
    decoder: &mut JBig2ArithDecoder<'_, '_>,
) -> Result<Vec<JBig2Image>, Jbig2Error> {
    let mut new_symbols = Vec::with_capacity(header.num_new_symbols);
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
        decode_arithmetic_symbol_width_run(header, height, decoder, &mut new_symbols)?;
    }

    Ok(new_symbols)
}

/// Decode one arithmetic symbol width run for a shared symbol height.
///
/// ITU-T T.88 / ISO/IEC 14492 section 7.4.2 terminates a width run with an
/// out-of-band result from the symbol width delta arithmetic integer context.
fn decode_arithmetic_symbol_width_run(
    header: &ParsedSymbolDictionaryHeader,
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
        let image = GenericRegion::new_arithmetic(
            width,
            height,
            GenericRegionTemplate::try_from(header.flags.sdtemplate())?,
            false,
            generic_at,
        )?
        .decode_arithmetic_with_decoder(decoder, None)?;
        new_symbols.push(image);
        if new_symbols.len() > header.num_new_symbols {
            return Err(Jbig2Error::InvalidState(SYMBOL_DICTIONARY_WIDTH_RUN));
        }
    }

    Ok(())
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
