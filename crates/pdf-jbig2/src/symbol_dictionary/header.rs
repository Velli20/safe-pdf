use super::flags::SymbolDictionaryFlagBits;
use crate::{
    error::Jbig2Error,
    generic_refinement_region::{RefinementAdaptiveTemplate, RefinementTemplate},
    generic_region::{GenericRegionAdaptiveTemplate, GenericRegionTemplate},
};
use pdf_utils::BitReader;

/// Parsed JBIG2 symbol dictionary header from T.88 / ISO 14492 section 7.4.2.1.
///
/// The header stores the symbol-dictionary flags, optional arithmetic generic
/// region adaptive-template bytes, and the declared exported/new symbol counts
/// needed to decode the segment body.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ParsedSymbolDictionaryHeader {
    /// Raw symbol-dictionary flags defined by section 7.4.2.1.1.
    pub(crate) flags: SymbolDictionaryFlagBits,
    /// Arithmetic generic-region adaptive template parsed when `SDHUFF` is not set.
    pub(super) generic_at: Option<GenericRegionAdaptiveTemplate>,
    /// Arithmetic refinement adaptive template parsed when `SDREFAGG` is set.
    pub(super) refinement_at: Option<RefinementAdaptiveTemplate>,
    /// Declared number of exported symbols produced by the segment.
    pub(crate) num_exported: usize,
    /// Declared number of newly decoded symbols carried by the segment.
    pub(crate) num_new_symbols: usize,
}

impl TryFrom<&mut BitReader<'_>> for ParsedSymbolDictionaryHeader {
    type Error = Jbig2Error;

    /// Parse a symbol dictionary header from the current byte-aligned stream position.
    ///
    /// This reads the fields defined by T.88 / ISO 14492 section 7.4.2.1. When
    /// the `SDHUFF` flag is clear, the arithmetic generic-region adaptive
    /// template bytes are present in the header and are parsed before the symbol
    /// counts. When `SDHUFF` is set, those bytes are omitted and `generic_at`
    /// remains `None`.
    fn try_from(stream: &mut BitReader<'_>) -> Result<Self, Self::Error> {
        let raw_flags = stream.try_read_u16_be::<u16>()?;
        let flags = SymbolDictionaryFlagBits::from_bits_retain(raw_flags);
        let generic_at = if flags.contains(SymbolDictionaryFlagBits::SDHUFF) {
            None
        } else {
            let template = GenericRegionTemplate::try_from(flags.sdtemplate())?;
            Some(GenericRegionAdaptiveTemplate::parse(
                stream, false, template,
            )?)
        };
        let refinement_at = if flags.contains(SymbolDictionaryFlagBits::SDREFAGG) {
            let template = RefinementTemplate::from_flag(
                flags.contains(SymbolDictionaryFlagBits::SDR_TEMPLATE),
            );
            if template == RefinementTemplate::Template0 {
                Some(RefinementAdaptiveTemplate::parse(stream, template)?)
            } else {
                Some(RefinementAdaptiveTemplate::default_for(template))
            }
        } else {
            None
        };

        Self {
            flags,
            generic_at,
            refinement_at,
            num_exported: stream.try_read_u32_be::<usize>()?,
            num_new_symbols: stream.try_read_u32_be::<usize>()?,
        }
        .validate_supported()
    }
}

impl ParsedSymbolDictionaryHeader {
    /// Validate the parsed header against the supported JBIG2 symbol-dictionary
    /// feature set.
    ///
    /// T.88 / ISO 14492 section 7.4.2.1 defines the symbol-dictionary header
    /// flags. This decoder rejects the invalid Huffman table selector value
    /// `2` for `SDHUFFDH` and `SDHUFFDW`.
    fn validate_supported(self) -> Result<Self, Jbig2Error> {
        if self.flags.contains(SymbolDictionaryFlagBits::SDHUFF)
            && (self.flags.sdhuffdh() == 2 || self.flags.sdhuffdw() == 2)
        {
            return Err(Jbig2Error::InvalidTable(
                "Huffman symbol dictionary selector",
            ));
        }

        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use super::ParsedSymbolDictionaryHeader;
    use crate::error::Jbig2Error;
    use pdf_utils::BitReader;

    #[test]
    fn arithmetic_header_reads_at_bytes_before_symbol_counts() {
        let flags = 0u16;
        let mut data = Vec::new();
        data.extend_from_slice(&flags.to_be_bytes());
        data.extend_from_slice(&[3u8, 0xff, 0xfd, 0xff, 2, 0xfe, 0xfe, 0xfe]);
        data.extend_from_slice(&7u32.to_be_bytes());
        data.extend_from_slice(&9u32.to_be_bytes());

        let mut reader = BitReader::new(&data);
        let header = ParsedSymbolDictionaryHeader::try_from(&mut reader).expect("header");
        let generic_at = header.generic_at.expect("AT");

        assert_eq!(header.num_exported, 7);
        assert_eq!(header.num_new_symbols, 9);
        assert_eq!(generic_at.normalized(), [3, -1, -3, -1, 2, -2, -2, -2]);
        assert_eq!(reader.byte_pos(), 18);
    }

    #[test]
    fn header_accepts_refinement_symbol_dictionaries() {
        let flags = 1u16 << 1;
        let mut data = Vec::new();
        data.extend_from_slice(&flags.to_be_bytes());
        data.extend_from_slice(&[0u8; 8]);
        data.extend_from_slice(&[0u8; 4]);
        data.extend_from_slice(&7u32.to_be_bytes());
        data.extend_from_slice(&9u32.to_be_bytes());

        let mut reader = BitReader::new(&data);
        let header = ParsedSymbolDictionaryHeader::try_from(&mut reader).expect("header");
        assert_eq!(header.num_exported, 7);
        assert_eq!(header.num_new_symbols, 9);
    }

    #[test]
    fn refinement_header_reads_refinement_at_bytes_before_symbol_counts() {
        let flags = 1u16 << 1;
        let mut data = Vec::new();
        data.extend_from_slice(&flags.to_be_bytes());
        data.extend_from_slice(&[3u8, 0xff, 0xfd, 0xff, 2, 0xfe, 0xfe, 0xfe]);
        data.extend_from_slice(&[0xff, 0xff, 0xff, 0xff]);
        data.extend_from_slice(&11u32.to_be_bytes());
        data.extend_from_slice(&4u32.to_be_bytes());

        let mut reader = BitReader::new(&data);
        let header = ParsedSymbolDictionaryHeader::try_from(&mut reader).expect("header");

        assert!(header.refinement_at.is_some());
        assert_eq!(header.num_exported, 11);
        assert_eq!(header.num_new_symbols, 4);
        assert_eq!(reader.byte_pos(), 22);
    }

    #[test]
    fn header_rejects_invalid_huffman_symbol_dictionary_selectors() {
        let flags = (1u16 << 0) | (2u16 << 2);
        let mut data = Vec::new();
        data.extend_from_slice(&flags.to_be_bytes());
        data.extend_from_slice(&7u32.to_be_bytes());
        data.extend_from_slice(&9u32.to_be_bytes());

        let mut reader = BitReader::new(&data);
        let err = ParsedSymbolDictionaryHeader::try_from(&mut reader).expect_err("header");
        assert_eq!(
            err,
            Jbig2Error::InvalidTable("Huffman symbol dictionary selector")
        );
    }

    #[test]
    fn header_accepts_custom_huffman_bitmap_size_tables() {
        let flags = (1u16 << 0) | (1u16 << 6);
        let mut data = Vec::new();
        data.extend_from_slice(&flags.to_be_bytes());
        data.extend_from_slice(&7u32.to_be_bytes());
        data.extend_from_slice(&9u32.to_be_bytes());

        let mut reader = BitReader::new(&data);
        let header = ParsedSymbolDictionaryHeader::try_from(&mut reader).expect("header");
        assert!(header.flags.sdhuffbmsize());
    }
}
