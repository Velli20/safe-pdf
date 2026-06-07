use crate::{error::Jbig2Error, image::JBig2Image};

const EXPORTED_SYMBOL_COUNT: &str = "exported symbol count";
const INPUT_SYMBOL: &str = "input";
const DECODED_SYMBOL: &str = "decoded";
const INTEGER_CONVERSION_OVERFLOW: &str = "integer conversion overflow";
const SYMBOL_EXPORT_RUN: &str = "symbol export run";

/// Export the selected symbols from the combined input and decoded symbol set.
///
/// ITU-T T.88 / ISO/IEC 14492 section 7.4.2 defines export flags over the
/// concatenation of referred input symbols followed by newly decoded symbols.
pub(super) fn export_dictionary_symbols(
    input_symbols: &[JBig2Image],
    new_symbols: &[JBig2Image],
    export_flags: &[bool],
    num_exported: usize,
) -> Result<Vec<JBig2Image>, Jbig2Error> {
    let exported_count = export_flags.iter().copied().filter(|flag| *flag).count();
    if exported_count > num_exported {
        return Err(Jbig2Error::InvalidState(EXPORTED_SYMBOL_COUNT));
    }

    let mut images = Vec::new();
    for (index, exported) in export_flags.iter().copied().enumerate() {
        if !exported || images.len() >= num_exported {
            continue;
        }
        images.push(symbol_by_export_index(input_symbols, new_symbols, index)?);
    }

    Ok(images)
}

/// Resolve one export index from the section 7.4.2 combined symbol sequence.
fn symbol_by_export_index(
    input_symbols: &[JBig2Image],
    new_symbols: &[JBig2Image],
    index: usize,
) -> Result<JBig2Image, Jbig2Error> {
    if index < input_symbols.len() {
        return input_symbols
            .get(index)
            .cloned()
            .ok_or(Jbig2Error::MissingSymbol(INPUT_SYMBOL));
    }

    let new_index = index
        .checked_sub(input_symbols.len())
        .ok_or(Jbig2Error::Overflow(INTEGER_CONVERSION_OVERFLOW))?;
    new_symbols
        .get(new_index)
        .cloned()
        .ok_or(Jbig2Error::MissingSymbol(DECODED_SYMBOL))
}

/// Return the total symbol count covered by export flags.
///
/// ITU-T T.88 / ISO/IEC 14492 section 7.4.2 export runs cover all referred
/// input symbols and all newly decoded symbols.
pub(super) fn total_symbol_count(
    input_symbol_count: usize,
    new_symbol_count: usize,
) -> Result<usize, Jbig2Error> {
    input_symbol_count
        .checked_add(new_symbol_count)
        .ok_or(Jbig2Error::Overflow(INTEGER_CONVERSION_OVERFLOW))
}

/// Fill one decoded export-flag run and return the next run start.
///
/// ITU-T T.88 / ISO/IEC 14492 section 7.4.2 encodes export flags as
/// alternating false/true runs starting with false.
pub(super) fn fill_export_flag_run(
    export_flags: &mut [bool],
    export_index: usize,
    run_length: usize,
    current_flag: bool,
) -> Result<usize, Jbig2Error> {
    let next_index = export_index
        .checked_add(run_length)
        .ok_or(Jbig2Error::Overflow(INTEGER_CONVERSION_OVERFLOW))?;
    if next_index > export_flags.len() {
        return Err(Jbig2Error::InvalidState(SYMBOL_EXPORT_RUN));
    }
    for flag in export_flags.iter_mut().take(next_index).skip(export_index) {
        *flag = current_flag;
    }

    Ok(next_index)
}

#[cfg(test)]
mod tests {
    use super::{export_dictionary_symbols, fill_export_flag_run};
    use crate::{error::Jbig2Error, image::JBig2Image};

    #[test]
    fn export_flag_run_rejects_run_past_symbol_count() {
        let mut export_flags = vec![false; 3];
        let err = fill_export_flag_run(&mut export_flags, 2, 2, true).expect_err("run error");
        assert_eq!(err, Jbig2Error::InvalidState("symbol export run"));
    }

    #[test]
    fn export_flag_run_allows_zero_length_runs() {
        let mut export_flags = vec![false; 2];
        let next = fill_export_flag_run(&mut export_flags, 1, 0, true).expect("run");

        assert_eq!(next, 1);
        assert_eq!(export_flags, [false, false]);
    }

    #[test]
    fn export_flag_run_sets_exact_requested_range() {
        let mut export_flags = vec![false; 4];
        let next = fill_export_flag_run(&mut export_flags, 1, 2, true).expect("run");

        assert_eq!(next, 3);
        assert_eq!(export_flags, [false, true, true, false]);
    }

    #[test]
    fn exported_dictionary_exports_all_declared_symbols() {
        let input_symbols = vec![JBig2Image::new(1, 1)];
        let new_symbols = vec![JBig2Image::new(2, 1), JBig2Image::new(3, 1)];
        let images =
            export_dictionary_symbols(&input_symbols, &new_symbols, &[true, true, true], 3)
                .expect("symbols");

        assert_eq!(images.len(), 3);
        assert_eq!(images.first().expect("first").width(), 1);
        assert_eq!(images.get(1).expect("second").width(), 2);
        assert_eq!(images.get(2).expect("third").width(), 3);
    }
}
