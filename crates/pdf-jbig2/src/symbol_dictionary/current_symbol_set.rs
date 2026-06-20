use crate::{error::Jbig2Error, image::JBig2Image};

/// Lookup view over the current symbol dictionary entries.
///
/// The symbol dictionary decoder resolves references against the referred
/// input symbols first and then against the newly decoded symbols in order.
#[derive(Debug, Clone, Copy)]
pub(crate) struct CurrentSymbolSet<'a> {
    input_symbols: &'a [JBig2Image],
    new_symbols: &'a [JBig2Image],
    invalid_symbol_label: &'static str,
}

impl<'a> CurrentSymbolSet<'a> {
    /// Build a lookup view over input and newly decoded symbols.
    pub(crate) fn new(
        input_symbols: &'a [JBig2Image],
        new_symbols: &'a [JBig2Image],
        invalid_symbol_label: &'static str,
    ) -> Self {
        Self {
            input_symbols,
            new_symbols,
            invalid_symbol_label,
        }
    }

    /// Resolve one symbol ID against the input and newly decoded symbol sets.
    pub(crate) fn get(&self, symbol_id: usize) -> Result<&'a JBig2Image, Jbig2Error> {
        if let Some(symbol) = self.input_symbols.get(symbol_id) {
            return Ok(symbol);
        }

        let new_index = symbol_id
            .checked_sub(self.input_symbols.len())
            .ok_or(Jbig2Error::InvalidState(self.invalid_symbol_label))?;
        self.new_symbols
            .get(new_index)
            .ok_or(Jbig2Error::InvalidState(self.invalid_symbol_label))
    }
}

#[cfg(test)]
mod tests {
    use super::CurrentSymbolSet;
    use crate::{error::Jbig2Error, image::JBig2Image};

    #[test]
    fn resolves_input_and_new_symbols() {
        let input_symbols = vec![JBig2Image::new(1, 1), JBig2Image::new(2, 1)];
        let new_symbols = vec![JBig2Image::new(3, 1), JBig2Image::new(4, 1)];
        let symbols = CurrentSymbolSet::new(&input_symbols, &new_symbols, "invalid");

        assert_eq!(symbols.get(0).expect("input").width(), 1);
        assert_eq!(symbols.get(1).expect("input").width(), 2);
        assert_eq!(symbols.get(2).expect("new").width(), 3);
        assert_eq!(symbols.get(3).expect("new").width(), 4);
    }

    #[test]
    fn reports_invalid_symbol_ids_with_the_caller_label() {
        let symbols = CurrentSymbolSet::new(&[], &[], "custom label");
        let err = symbols.get(0).expect_err("invalid symbol");

        assert_eq!(err, Jbig2Error::InvalidState("custom label"));
    }
}
