use crate::error::Jbig2Error;
use bitflags::bitflags;
use pdf_utils::BitReader;

const HD_TEMPLATE_SHIFT: u8 = 1;
const HD_TEMPLATE_VALUE_MASK: u8 = 0b11;
const PATTERN_COUNT_INCREMENT: u32 = 1;

bitflags! {
    /// Raw JBIG2 pattern dictionary flags from T.88 / ISO 14492 section 7.4.4.1.1.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    struct PatternDictionaryFlagBits: u8 {
        /// `HDMMR`: whether the collective bitmap is MMR encoded.
        const HDMMR = 1 << 0;
        /// `HDTEMPLATE`: arithmetic generic-region template selector.
        const HDTEMPLATE_MASK = HD_TEMPLATE_VALUE_MASK << HD_TEMPLATE_SHIFT;
    }
}

impl PatternDictionaryFlagBits {
    /// Return the `HDTEMPLATE` value from section 7.4.4.1.1.
    fn hd_template(self) -> u8 {
        (self.bits() >> HD_TEMPLATE_SHIFT) & HD_TEMPLATE_VALUE_MASK
    }
}

/// Parsed JBIG2 pattern dictionary header.
///
/// ITU-T T.88 / ISO/IEC 14492 section 7.4.4.1 defines this header as the
/// flags byte, pattern dimensions, and highest pattern index (`GRAYMAX`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PatternDictionaryHeader {
    mmr: bool,
    template: u8,
    pattern_width: u8,
    pattern_height: u8,
    max_pattern_index: u32,
}

impl PatternDictionaryHeader {
    /// Parse a pattern dictionary header from a byte-aligned JBIG2 stream.
    ///
    /// This consumes the section 7.4.4.1 fields `HDMMR`, `HDTEMPLATE`,
    /// `HDPW`, `HDPH`, and `GRAYMAX`, leaving the stream positioned at the
    /// collective bitmap data.
    pub(crate) fn parse(stream: &mut BitReader<'_>) -> Result<Self, Jbig2Error> {
        Self::try_from(stream)
    }

    /// Return whether the collective bitmap is MMR encoded (`HDMMR`).
    pub(crate) const fn mmr(self) -> bool {
        self.mmr
    }

    /// Return the arithmetic generic-region template selector (`HDTEMPLATE`).
    pub(crate) const fn template(self) -> u8 {
        self.template
    }

    /// Return the width in pixels of each pattern (`HDPW`).
    pub(crate) const fn pattern_width(self) -> u8 {
        self.pattern_width
    }

    /// Return the height in pixels of each pattern (`HDPH`).
    pub(crate) const fn pattern_height(self) -> u8 {
        self.pattern_height
    }

    /// Return the highest pattern index declared by the dictionary (`GRAYMAX`).
    #[cfg(test)]
    pub(crate) const fn max_pattern_index(self) -> u32 {
        self.max_pattern_index
    }

    /// Return the number of decoded patterns implied by `GRAYMAX`.
    ///
    /// Section 7.4.4 defines pattern indices from zero through `GRAYMAX`,
    /// making the count `GRAYMAX + 1`.
    pub(crate) fn pattern_count(self) -> Result<u32, Jbig2Error> {
        self.max_pattern_index
            .checked_add(PATTERN_COUNT_INCREMENT)
            .ok_or(Jbig2Error::Overflow("pattern count overflow"))
    }
}

impl TryFrom<&mut BitReader<'_>> for PatternDictionaryHeader {
    type Error = Jbig2Error;

    /// Parse a pattern dictionary header from the current stream position.
    ///
    /// The wire layout is defined by T.88 / ISO 14492 section 7.4.4.1:
    /// one flags byte, one byte each for `HDPW` and `HDPH`, and a big-endian
    /// 32-bit `GRAYMAX` value.
    fn try_from(stream: &mut BitReader<'_>) -> Result<Self, Self::Error> {
        let raw_flags = stream.try_read_u8::<u8>()?;
        let pattern_width = stream.try_read_u8::<u8>()?;
        let pattern_height = stream.try_read_u8::<u8>()?;
        let max_pattern_index = stream.try_read_u32_be::<u32>()?;
        let flags = PatternDictionaryFlagBits::from_bits_retain(raw_flags);

        Ok(Self {
            mmr: flags.contains(PatternDictionaryFlagBits::HDMMR),
            template: flags.hd_template(),
            pattern_width,
            pattern_height,
            max_pattern_index,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::PatternDictionaryHeader;
    use crate::error::Jbig2Error;
    use pdf_utils::BitReader;

    #[test]
    fn parses_pattern_dictionary_header() {
        let data = [0x04, 0x03, 0x02, 0x00, 0x00, 0x00, 0x05];
        let mut stream = BitReader::new(&data);
        let header = PatternDictionaryHeader::parse(&mut stream).expect("header");

        assert!(!header.mmr());
        assert_eq!(header.template(), 2);
        assert_eq!(header.pattern_width(), 3);
        assert_eq!(header.pattern_height(), 2);
        assert_eq!(header.max_pattern_index(), 5);
        assert_eq!(header.pattern_count().expect("count"), 6);
        assert_eq!(stream.byte_pos(), 7);
    }

    #[test]
    fn pattern_count_rejects_graymax_overflow() {
        let data = [0x00, 0x03, 0x02, 0xff, 0xff, 0xff, 0xff];
        let mut stream = BitReader::new(&data);
        let header = PatternDictionaryHeader::parse(&mut stream).expect("header");

        assert_eq!(
            header.pattern_count().expect_err("overflow"),
            Jbig2Error::Overflow("pattern count overflow")
        );
    }
}
