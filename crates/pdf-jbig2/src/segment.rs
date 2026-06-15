//! JBIG2 segment metadata and decoded results.

use crate::pattern_dictionary::PatternDictionary;
use crate::symbol_dictionary::SymbolDictionary;

use super::huffman::CustomHuffmanDecoder;
use super::image::JBig2Image;
use super::segment_header::SegmentHeaderFlagBits;

/// JBIG2 segment type codes from T.88 / ISO 14492 section 7.2.3.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum SegmentType {
    SymbolDictionary,
    IntermediateTextRegion,
    ImmediateTextRegion,
    ImmediateLosslessTextRegion,
    PatternDictionary,
    IntermediateHalftoneRegion,
    ImmediateHalftoneRegion,
    ImmediateLosslessHalftoneRegion,
    IntermediateGenericRegion,
    ImmediateGenericRegion,
    ImmediateLosslessGenericRegion,
    IntermediateGenericRefinementRegion,
    ImmediateGenericRefinementRegion,
    ImmediateLosslessGenericRefinementRegion,
    PageInformation,
    EndOfPage,
    EndOfStripe,
    EndOfFile,
    Profile,
    CodeTable,
    Extension,
}

impl SegmentType {
    pub(crate) fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::SymbolDictionary),
            4 => Some(Self::IntermediateTextRegion),
            6 => Some(Self::ImmediateTextRegion),
            7 => Some(Self::ImmediateLosslessTextRegion),
            16 => Some(Self::PatternDictionary),
            20 => Some(Self::IntermediateHalftoneRegion),
            22 => Some(Self::ImmediateHalftoneRegion),
            23 => Some(Self::ImmediateLosslessHalftoneRegion),
            36 => Some(Self::IntermediateGenericRegion),
            38 => Some(Self::ImmediateGenericRegion),
            39 => Some(Self::ImmediateLosslessGenericRegion),
            40 => Some(Self::IntermediateGenericRefinementRegion),
            42 => Some(Self::ImmediateGenericRefinementRegion),
            43 => Some(Self::ImmediateLosslessGenericRefinementRegion),
            48 => Some(Self::PageInformation),
            49 => Some(Self::EndOfPage),
            50 => Some(Self::EndOfStripe),
            51 => Some(Self::EndOfFile),
            52 => Some(Self::Profile),
            53 => Some(Self::CodeTable),
            62 => Some(Self::Extension),
            _ => None,
        }
    }

    #[cfg(test)]
    pub(crate) fn code(self) -> u8 {
        match self {
            Self::SymbolDictionary => 0,
            Self::IntermediateTextRegion => 4,
            Self::ImmediateTextRegion => 6,
            Self::ImmediateLosslessTextRegion => 7,
            Self::PatternDictionary => 16,
            Self::IntermediateHalftoneRegion => 20,
            Self::ImmediateHalftoneRegion => 22,
            Self::ImmediateLosslessHalftoneRegion => 23,
            Self::IntermediateGenericRegion => 36,
            Self::ImmediateGenericRegion => 38,
            Self::ImmediateLosslessGenericRegion => 39,
            Self::IntermediateGenericRefinementRegion => 40,
            Self::ImmediateGenericRefinementRegion => 42,
            Self::ImmediateLosslessGenericRefinementRegion => 43,
            Self::PageInformation => 48,
            Self::EndOfPage => 49,
            Self::EndOfStripe => 50,
            Self::EndOfFile => 51,
            Self::Profile => 52,
            Self::CodeTable => 53,
            Self::Extension => 62,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum JBig2SegmentResult {
    None,
    HuffmanTable(CustomHuffmanDecoder),
    Image(JBig2Image),
    PatternDictionary(PatternDictionary),
    SymbolDictionary(SymbolDictionary),
}

impl Default for JBig2SegmentResult {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct ParsedSegment {
    pub(crate) number: u32,
    pub(crate) flags: u8,
    pub(crate) referred_to_segment_numbers: Vec<u32>,
    pub(crate) page_association: u32,
    pub(crate) data_length: Option<usize>,
    pub(crate) result: JBig2SegmentResult,
}

impl ParsedSegment {
    pub(crate) fn flags_type(&self) -> u8 {
        SegmentHeaderFlagBits::from_bits_retain(self.flags).segment_type()
    }

    pub(crate) fn segment_type(&self) -> Option<SegmentType> {
        SegmentType::from_code(self.flags_type())
    }
}

#[cfg(test)]
mod tests {
    use super::SegmentType;

    #[test]
    fn maps_segment_type_codes() {
        assert_eq!(
            SegmentType::from_code(0),
            Some(SegmentType::SymbolDictionary)
        );
        assert_eq!(
            SegmentType::from_code(48),
            Some(SegmentType::PageInformation)
        );
        assert_eq!(
            SegmentType::from_code(20),
            Some(SegmentType::IntermediateHalftoneRegion)
        );
        assert_eq!(
            SegmentType::from_code(40),
            Some(SegmentType::IntermediateGenericRefinementRegion)
        );
        assert_eq!(
            SegmentType::from_code(42),
            Some(SegmentType::ImmediateGenericRefinementRegion)
        );
        assert_eq!(
            SegmentType::from_code(43),
            Some(SegmentType::ImmediateLosslessGenericRefinementRegion)
        );
        assert_eq!(SegmentType::from_code(63), None);
        assert_eq!(SegmentType::PageInformation.code(), 48);
    }
}
