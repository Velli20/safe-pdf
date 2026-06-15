use crate::{
    error::Jbig2Error,
    huffman::{
        StandardHuffmanDecoder,
        standard::{
            STANDARD_TABLE_B1, STANDARD_TABLE_B2, STANDARD_TABLE_B3, STANDARD_TABLE_B4,
            STANDARD_TABLE_B5, STANDARD_TABLE_B6, STANDARD_TABLE_B7, STANDARD_TABLE_B8,
            STANDARD_TABLE_B9, STANDARD_TABLE_B10, STANDARD_TABLE_B11, STANDARD_TABLE_B12,
            STANDARD_TABLE_B13, STANDARD_TABLE_B14, STANDARD_TABLE_B15,
        },
    },
};

const CUSTOM_SYMBOL_DICTIONARY_TABLES: &str = "custom Huffman symbol dictionary tables";
const CUSTOM_TEXT_REGION_TABLES: &str = "custom text-region Huffman tables";
const CUSTOM_TEXT_REGION_REFINEMENT_TABLES: &str = "custom text-region refinement Huffman tables";
const CUSTOM_TEXT_REGION_RSIZE_TABLES: &str = "custom text-region refinement-size Huffman tables";
const SELECTOR_STANDARD_ZERO: u8 = 0;
const SELECTOR_STANDARD_ONE: u8 = 1;
const SELECTOR_STANDARD_TWO: u8 = 2;

/// Huffman table selector field from a JBIG2 segment header.
///
/// The variants correspond to selector fields defined by ITU-T T.88 /
/// ISO/IEC 14492 section 7.4.2.1 for symbol dictionaries and section 7.4.3.1
/// for text regions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HuffmanTableSelection {
    /// `SDHUFFDH` selector for symbol dictionary delta height.
    SymbolDictionaryDh(u8),
    /// `SDHUFFDW` selector for symbol dictionary delta width.
    SymbolDictionaryDw(u8),
    /// `SBHUFFFS` selector for text-region first symbol position.
    TextRegionFs(u8),
    /// `SBHUFFDS` selector for text-region symbol position deltas.
    TextRegionDs(u8),
    /// `SBHUFFDT` selector for text-region symbol `T` deltas.
    TextRegionDt(u8),
}

impl HuffmanTableSelection {
    /// Build the selected standard Huffman decoder.
    ///
    /// Selector values that reference custom Huffman tables are rejected
    /// because this decoder currently supports only the standard tables from
    /// ITU-T T.88 / ISO/IEC 14492 Annex B.
    pub(crate) fn standard_decoder(self) -> Result<StandardHuffmanDecoder, Jbig2Error> {
        let table_id = match self {
            Self::SymbolDictionaryDh(SELECTOR_STANDARD_ZERO) => STANDARD_TABLE_B4,
            Self::SymbolDictionaryDh(SELECTOR_STANDARD_ONE) => STANDARD_TABLE_B5,
            Self::SymbolDictionaryDw(SELECTOR_STANDARD_ZERO) => STANDARD_TABLE_B2,
            Self::SymbolDictionaryDw(SELECTOR_STANDARD_ONE) => STANDARD_TABLE_B3,
            Self::TextRegionFs(SELECTOR_STANDARD_ZERO) => STANDARD_TABLE_B6,
            Self::TextRegionFs(SELECTOR_STANDARD_ONE) => STANDARD_TABLE_B7,
            Self::TextRegionDs(SELECTOR_STANDARD_ZERO) => STANDARD_TABLE_B8,
            Self::TextRegionDs(SELECTOR_STANDARD_ONE) => STANDARD_TABLE_B9,
            Self::TextRegionDs(SELECTOR_STANDARD_TWO) => STANDARD_TABLE_B10,
            Self::TextRegionDt(SELECTOR_STANDARD_ZERO) => STANDARD_TABLE_B11,
            Self::TextRegionDt(SELECTOR_STANDARD_ONE) => STANDARD_TABLE_B12,
            Self::TextRegionDt(SELECTOR_STANDARD_TWO) => STANDARD_TABLE_B13,
            Self::SymbolDictionaryDh(_) | Self::SymbolDictionaryDw(_) => {
                return Err(Jbig2Error::UnsupportedFeature(
                    CUSTOM_SYMBOL_DICTIONARY_TABLES,
                ));
            }
            Self::TextRegionFs(_) | Self::TextRegionDs(_) | Self::TextRegionDt(_) => {
                return Err(Jbig2Error::UnsupportedFeature(CUSTOM_TEXT_REGION_TABLES));
            }
        };
        StandardHuffmanDecoder::new(table_id)
    }
}

/// Build the standard Huffman decoder for a text-region refinement delta.
///
/// ITU-T T.88 / ISO/IEC 14492 section 7.4.3.1.2 uses the standard tables
/// from Annex B when the selector is `0` or `1`; custom tables are not
/// supported by this decoder.
pub(crate) fn text_region_refinement_standard_decoder(
    selector: u8,
) -> Result<StandardHuffmanDecoder, Jbig2Error> {
    let table_id = match selector {
        SELECTOR_STANDARD_ZERO => STANDARD_TABLE_B14,
        SELECTOR_STANDARD_ONE => STANDARD_TABLE_B15,
        _ => {
            return Err(Jbig2Error::UnsupportedFeature(
                CUSTOM_TEXT_REGION_REFINEMENT_TABLES,
            ));
        }
    };
    StandardHuffmanDecoder::new(table_id)
}

/// Build the standard Huffman decoder for a text-region refinement-size value.
///
/// ITU-T T.88 / ISO/IEC 14492 section 7.4.3.1.2 uses standard table B.1 when
/// `SBHUFFRSIZE = 0`. Custom refinement-size tables are not supported.
pub(crate) fn text_region_rsize_standard_decoder(
    custom: bool,
) -> Result<StandardHuffmanDecoder, Jbig2Error> {
    if custom {
        return Err(Jbig2Error::UnsupportedFeature(
            CUSTOM_TEXT_REGION_RSIZE_TABLES,
        ));
    }
    StandardHuffmanDecoder::new(STANDARD_TABLE_B1)
}

#[cfg(test)]
mod tests {
    use crate::{
        error::Jbig2Error,
        huffman::{
            HuffmanTableSelection, StandardHuffmanDecoder,
            standard::{
                STANDARD_TABLE_B1, STANDARD_TABLE_B2, STANDARD_TABLE_B3, STANDARD_TABLE_B4,
                STANDARD_TABLE_B5, STANDARD_TABLE_B6, STANDARD_TABLE_B7, STANDARD_TABLE_B8,
                STANDARD_TABLE_B9, STANDARD_TABLE_B10, STANDARD_TABLE_B11, STANDARD_TABLE_B12,
                STANDARD_TABLE_B13, STANDARD_TABLE_B14, STANDARD_TABLE_B15,
            },
            text_region_refinement_standard_decoder, text_region_rsize_standard_decoder,
        },
    };

    #[test]
    fn selects_expected_standard_tables() {
        let cases = [
            (
                HuffmanTableSelection::SymbolDictionaryDh(0),
                STANDARD_TABLE_B4,
            ),
            (
                HuffmanTableSelection::SymbolDictionaryDh(1),
                STANDARD_TABLE_B5,
            ),
            (
                HuffmanTableSelection::SymbolDictionaryDw(0),
                STANDARD_TABLE_B2,
            ),
            (
                HuffmanTableSelection::SymbolDictionaryDw(1),
                STANDARD_TABLE_B3,
            ),
            (HuffmanTableSelection::TextRegionFs(0), STANDARD_TABLE_B6),
            (HuffmanTableSelection::TextRegionFs(1), STANDARD_TABLE_B7),
            (HuffmanTableSelection::TextRegionDs(0), STANDARD_TABLE_B8),
            (HuffmanTableSelection::TextRegionDs(1), STANDARD_TABLE_B9),
            (HuffmanTableSelection::TextRegionDs(2), STANDARD_TABLE_B10),
            (HuffmanTableSelection::TextRegionDt(0), STANDARD_TABLE_B11),
            (HuffmanTableSelection::TextRegionDt(1), STANDARD_TABLE_B12),
            (HuffmanTableSelection::TextRegionDt(2), STANDARD_TABLE_B13),
        ];

        for (selector, expected) in cases {
            let selected = selector.standard_decoder().expect("selected table");
            let standard = StandardHuffmanDecoder::new(expected).expect("standard table");
            assert_eq!(selected, standard);
        }
    }

    #[test]
    fn rejects_unsupported_symbol_dictionary_selectors() {
        let result = HuffmanTableSelection::SymbolDictionaryDh(2).standard_decoder();
        assert!(matches!(
            result,
            Err(Jbig2Error::UnsupportedFeature(message))
                if message == super::CUSTOM_SYMBOL_DICTIONARY_TABLES
        ));
    }

    #[test]
    fn rejects_unsupported_text_region_selectors() {
        let result = HuffmanTableSelection::TextRegionDs(3).standard_decoder();
        assert!(matches!(
            result,
            Err(Jbig2Error::UnsupportedFeature(message))
                if message == super::CUSTOM_TEXT_REGION_TABLES
        ));
    }

    #[test]
    fn selects_expected_text_region_refinement_tables() {
        let cases = [(0, STANDARD_TABLE_B14), (1, STANDARD_TABLE_B15)];

        for (selector, expected) in cases {
            let selected = text_region_refinement_standard_decoder(selector).expect("selected");
            let standard = StandardHuffmanDecoder::new(expected).expect("standard table");
            assert_eq!(selected, standard);
        }
    }

    #[test]
    fn rejects_custom_text_region_refinement_selectors() {
        let result = text_region_refinement_standard_decoder(2);
        assert!(matches!(
            result,
            Err(Jbig2Error::UnsupportedFeature(message))
                if message == super::CUSTOM_TEXT_REGION_REFINEMENT_TABLES
        ));
    }

    #[test]
    fn selects_standard_rsize_table_b1() {
        let selected = text_region_rsize_standard_decoder(false).expect("selected");
        let standard = StandardHuffmanDecoder::new(STANDARD_TABLE_B1).expect("standard table");
        assert_eq!(selected, standard);
    }

    #[test]
    fn rejects_custom_rsize_table() {
        let result = text_region_rsize_standard_decoder(true);
        assert!(matches!(
            result,
            Err(Jbig2Error::UnsupportedFeature(message))
                if message == super::CUSTOM_TEXT_REGION_RSIZE_TABLES
        ));
    }
}
