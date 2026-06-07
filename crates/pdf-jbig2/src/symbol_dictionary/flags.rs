use bitflags::bitflags;

const SDHUFF_BIT: u16 = 0;
const SDREFAGG_BIT: u16 = 1;
const SDHUFF_DH_SHIFT: u16 = 2;
const SDHUFF_DW_SHIFT: u16 = 4;
const SDHUFF_BMSIZE_BIT: u16 = 6;
const SDHUFF_AGGINST_BIT: u16 = 7;
const SD_TEMPLATE_SHIFT: u16 = 10;
const SDR_TEMPLATE_BIT: u16 = 12;
const TWO_BIT_SELECTOR_MASK: u16 = 0b11;

bitflags! {
    /// Symbol dictionary flags from ITU-T T.88 / ISO/IEC 14492 section 7.4.2.1.1.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub(super) struct SymbolDictionaryFlagBits: u16 {
        /// `SDHUFF`: selects Huffman coding when set and arithmetic coding when clear.
        const SDHUFF = 1 << SDHUFF_BIT;
        /// `SDREFAGG`: enables refinement/aggregate-coded symbols.
        const SDREFAGG = 1 << SDREFAGG_BIT;
        /// `SDHUFFDH`: two-bit selector for the symbol height delta Huffman table.
        const SDHUFF_DH_MASK = TWO_BIT_SELECTOR_MASK << SDHUFF_DH_SHIFT;
        /// `SDHUFFDW`: two-bit selector for the symbol width delta Huffman table.
        const SDHUFF_DW_MASK = TWO_BIT_SELECTOR_MASK << SDHUFF_DW_SHIFT;
        /// `SDHUFFBMSIZE`: selects the collective bitmap-size Huffman table.
        const SDHUFF_BMSIZE = 1 << SDHUFF_BMSIZE_BIT;
        /// `SDHUFFAGGINST`: selects the aggregate-instance Huffman table.
        const SDHUFF_AGGINST = 1 << SDHUFF_AGGINST_BIT;
        /// `SDTEMPLATE`: two-bit selector for the generic-region template.
        const SD_TEMPLATE_MASK = TWO_BIT_SELECTOR_MASK << SD_TEMPLATE_SHIFT;
        /// `SDRTEMPLATE`: refinement generic-region template selector.
        const SDR_TEMPLATE = 1 << SDR_TEMPLATE_BIT;
    }
}

impl SymbolDictionaryFlagBits {
    /// Return `SDHUFFDH`, the delta-height Huffman selector from section 7.4.2.1.1.
    pub(super) fn sdhuffdh(self) -> u8 {
        self.two_bit_selector(Self::SDHUFF_DH_MASK, SDHUFF_DH_SHIFT)
    }

    /// Return `SDHUFFDW`, the delta-width Huffman selector from section 7.4.2.1.1.
    pub(super) fn sdhuffdw(self) -> u8 {
        self.two_bit_selector(Self::SDHUFF_DW_MASK, SDHUFF_DW_SHIFT)
    }

    /// Return whether `SDHUFFBMSIZE` is set as defined by section 7.4.2.1.1.
    pub(super) fn sdhuffbmsize(self) -> bool {
        self.contains(Self::SDHUFF_BMSIZE)
    }

    /// Return `SDTEMPLATE`, the generic-region template selector from section 7.4.2.1.1.
    pub(super) fn sdtemplate(self) -> u8 {
        self.two_bit_selector(Self::SD_TEMPLATE_MASK, SD_TEMPLATE_SHIFT)
    }

    /// Extract a two-bit selector field defined by section 7.4.2.1.1.
    fn two_bit_selector(self, mask: Self, shift: u16) -> u8 {
        u8::try_from((self.bits() & mask.bits()) >> shift).map_or(0, |value| value)
    }
}

#[cfg(test)]
mod tests {
    use super::{SDHUFF_AGGINST_BIT, SDR_TEMPLATE_BIT, SymbolDictionaryFlagBits};

    #[test]
    fn extracts_symbol_dictionary_flags() {
        let bits = SymbolDictionaryFlagBits::from_bits_retain(
            SymbolDictionaryFlagBits::SDHUFF.bits()
                | SymbolDictionaryFlagBits::SDREFAGG.bits()
                | SymbolDictionaryFlagBits::SDHUFF_DH_MASK.bits()
                | SymbolDictionaryFlagBits::SDHUFF_BMSIZE.bits()
                | (1u16 << SDHUFF_AGGINST_BIT),
        );

        assert!(bits.contains(SymbolDictionaryFlagBits::SDHUFF));
        assert!(bits.contains(SymbolDictionaryFlagBits::SDREFAGG));
        assert_eq!(bits.sdhuffdh(), 3);
        assert_eq!(bits.sdhuffdw(), 0);
        assert!(bits.sdhuffbmsize());
        assert_eq!(bits.sdtemplate(), 0);
        assert!(!bits.contains(SymbolDictionaryFlagBits::SDR_TEMPLATE));
    }

    #[test]
    fn extracts_refinement_template_flag() {
        let bits = SymbolDictionaryFlagBits::from_bits_retain(1u16 << SDR_TEMPLATE_BIT);

        assert!(bits.contains(SymbolDictionaryFlagBits::SDR_TEMPLATE));
    }
}
