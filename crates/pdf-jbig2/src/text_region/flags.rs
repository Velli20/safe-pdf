//! JBIG2 text-region flag parsing.

use bitflags::bitflags;

const LOG_SB_STRIPS_SHIFT: u16 = 2;
const REF_CORNER_SHIFT: u16 = 4;
const SB_COMB_OP_SHIFT: u16 = 7;
const SB_DS_OFFSET_SHIFT: u16 = 10;
const SB_DS_OFFSET_VALUES: [i8; 32] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, -16, -15, -14, -13, -12, -11, -10, -9,
    -8, -7, -6, -5, -4, -3, -2, -1,
];

bitflags! {
    /// Text-region flags from ITU-T T.88 | ISO/IEC 14492 section 7.4.3.1.1.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub(crate) struct TextRegionFlagBits: u16 {
        /// `SBHUFF`: selects Huffman coding when set, arithmetic coding otherwise.
        const SBHUFF = 1 << 0;
        /// `SBREFINE`: enables refinement coding for symbol instances.
        const SBREFINE = 1 << 1;
        /// Encoded `LOGSBSTRIPS` field from Table 9.
        const LOG_SB_STRIPS_MASK = 0b11 << LOG_SB_STRIPS_SHIFT;
        /// Encoded `REFCORNER` field from Table 9.
        const REF_CORNER_MASK = 0b11 << REF_CORNER_SHIFT;
        /// `TRANSPOSED`: swaps the text-region `S` and `T` axes.
        const TRANSPOSED = 1 << 6;
        /// Encoded `SBCOMBOP` composition operator from Table 9.
        const SB_COMB_OP_MASK = 0b11 << SB_COMB_OP_SHIFT;
        /// `SBDEFPIXEL`: default pixel used to initialize the region bitmap.
        const SBDEFPIXEL = 1 << 9;
        /// Encoded signed five-bit `SBDSOFFSET` field from Table 9.
        const SB_DS_OFFSET_MASK = 0b1_1111 << SB_DS_OFFSET_SHIFT;
        /// `SBRTEMPLATE`: refinement template selector from Table 9.
        const SBRTEMPLATE = 1 << 15;
    }
}

impl TextRegionFlagBits {
    /// Return `SBSTRIPS = 2^LOGSBSTRIPS`.
    ///
    /// ITU-T T.88 | ISO/IEC 14492 section 7.4.3.1.1 stores the logarithm of
    /// the strip count in Table 9, while section 6.4.5 uses the decoded strip
    /// count in `STRIPT` updates.
    pub(crate) fn sbstrips(self) -> u8 {
        1u8 << self.field_u8(Self::LOG_SB_STRIPS_MASK, LOG_SB_STRIPS_SHIFT)
    }

    /// Return the encoded `REFCORNER` selector.
    ///
    /// ITU-T T.88 | ISO/IEC 14492 section 7.4.3.1.1 Table 9 defines this
    /// two-bit field; geometry code maps it to the reference-corner enum.
    pub(crate) fn refcorner(self) -> u8 {
        self.field_u8(Self::REF_CORNER_MASK, REF_CORNER_SHIFT)
    }

    /// Return the encoded `SBCOMBOP` composition operator.
    ///
    /// ITU-T T.88 | ISO/IEC 14492 section 7.4.3.1.1 Table 9 stores the
    /// operator used when symbol instances are composed into the region.
    pub(crate) fn sbcombop_bits(self) -> u8 {
        self.field_u8(Self::SB_COMB_OP_MASK, SB_COMB_OP_SHIFT)
    }

    /// Return the signed five-bit `SBDSOFFSET` value.
    ///
    /// ITU-T T.88 | ISO/IEC 14492 section 7.4.3.1.1 Table 9 defines this
    /// signed offset for subsequent-symbol `S` deltas in section 6.4.8.
    pub(crate) fn sbdsoffset(self) -> i8 {
        let raw = self.field_u8(Self::SB_DS_OFFSET_MASK, SB_DS_OFFSET_SHIFT);
        SB_DS_OFFSET_VALUES
            .get(usize::from(raw))
            .copied()
            .unwrap_or(0)
    }

    fn field_u8(self, mask: Self, shift: u16) -> u8 {
        let value = (self.bits() & mask.bits()) >> shift;
        let [low, _] = value.to_le_bytes();
        low
    }
}

#[cfg(test)]
mod tests {
    use super::TextRegionFlagBits;
    use crate::compose_op::ComposeOp;

    #[test]
    fn extracts_text_region_flags() {
        let bits = TextRegionFlagBits::from_bits_retain(
            1 | (1 << 1)
                | (2 << 2)
                | (3 << 4)
                | (1 << 6)
                | (2 << 7)
                | (1 << 9)
                | (0x1f << 10)
                | (1 << 15),
        );
        assert!(bits.contains(TextRegionFlagBits::SBHUFF));
        assert!(bits.contains(TextRegionFlagBits::SBREFINE));
        assert_eq!(bits.sbstrips(), 4);
        assert_eq!(bits.refcorner(), 3);
        assert!(bits.contains(TextRegionFlagBits::TRANSPOSED));
        assert_eq!(bits.sbcombop_bits(), 2);
        assert!(bits.contains(TextRegionFlagBits::SBDEFPIXEL));
        assert_eq!(bits.sbdsoffset(), -1);
        assert!(bits.contains(TextRegionFlagBits::SBRTEMPLATE));
        assert_eq!(ComposeOp::from(bits.sbcombop_bits()), ComposeOp::Xor);
    }
}
