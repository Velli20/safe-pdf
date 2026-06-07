//! JBIG2 text-region Huffman flag parsing.

use bitflags::bitflags;

use crate::error::Jbig2Error;

const SBHUFF_FS_SHIFT: u16 = 0;
const SBHUFF_DS_SHIFT: u16 = 2;
const SBHUFF_DT_SHIFT: u16 = 4;
const SBHUFF_RDW_SHIFT: u16 = 6;
const SBHUFF_RDH_SHIFT: u16 = 8;
const SBHUFF_RDX_SHIFT: u16 = 10;
const SBHUFF_RDY_SHIFT: u16 = 12;

bitflags! {
    /// Text-region Huffman flags from ITU-T T.88 | ISO/IEC 14492 section 7.4.3.1.2.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    struct TextRegionHuffmanFlagBits: u16 {
        /// `SBHUFFFS`: first-symbol `S` delta table selector.
        const SBHUFF_FS_MASK = 0b11 << SBHUFF_FS_SHIFT;
        /// `SBHUFFDS`: subsequent-symbol `S` delta table selector.
        const SBHUFF_DS_MASK = 0b11 << SBHUFF_DS_SHIFT;
        /// `SBHUFFDT`: strip `T` delta table selector.
        const SBHUFF_DT_MASK = 0b11 << SBHUFF_DT_SHIFT;
        /// `SBHUFFRDW`: refinement width delta table selector.
        const SBHUFF_RDW_MASK = 0b11 << SBHUFF_RDW_SHIFT;
        /// `SBHUFFRDH`: refinement height delta table selector.
        const SBHUFF_RDH_MASK = 0b11 << SBHUFF_RDH_SHIFT;
        /// `SBHUFFRDX`: refinement x delta table selector.
        const SBHUFF_RDX_MASK = 0b11 << SBHUFF_RDX_SHIFT;
        /// `SBHUFFRDY`: refinement y delta table selector.
        const SBHUFF_RDY_MASK = 0b11 << SBHUFF_RDY_SHIFT;
        /// `SBHUFFRSIZE`: selects a custom refinement-size table.
        const SBHUFF_RSIZE = 1 << 14;
        /// Reserved bit 15 from section 7.4.3.1.2.
        const RESERVED_15 = 1 << 15;
    }
}

impl TextRegionHuffmanFlagBits {
    fn sbhufffs(self) -> u8 {
        self.field_u8(Self::SBHUFF_FS_MASK, SBHUFF_FS_SHIFT)
    }

    fn sbhuffds(self) -> u8 {
        self.field_u8(Self::SBHUFF_DS_MASK, SBHUFF_DS_SHIFT)
    }

    fn sbhuffdt(self) -> u8 {
        self.field_u8(Self::SBHUFF_DT_MASK, SBHUFF_DT_SHIFT)
    }

    fn sbhuffrdw(self) -> u8 {
        self.field_u8(Self::SBHUFF_RDW_MASK, SBHUFF_RDW_SHIFT)
    }

    fn sbhuffrdh(self) -> u8 {
        self.field_u8(Self::SBHUFF_RDH_MASK, SBHUFF_RDH_SHIFT)
    }

    fn sbhuffrdx(self) -> u8 {
        self.field_u8(Self::SBHUFF_RDX_MASK, SBHUFF_RDX_SHIFT)
    }

    fn sbhuffrdy(self) -> u8 {
        self.field_u8(Self::SBHUFF_RDY_MASK, SBHUFF_RDY_SHIFT)
    }

    fn rsize_custom(self) -> bool {
        self.contains(Self::SBHUFF_RSIZE)
    }

    fn field_u8(self, mask: Self, shift: u16) -> u8 {
        let value = (self.bits() & mask.bits()) >> shift;
        let [low, _] = value.to_le_bytes();
        low
    }
}

/// Parsed text-region Huffman table selectors.
///
/// ITU-T T.88 | ISO/IEC 14492 section 7.4.3.1.2 stores these selectors after
/// the text-region flags when `SBHUFF = 1`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TextRegionHuffmanFlags {
    /// `SBHUFFFS`: selector for section 6.4.7 first-symbol `S` deltas.
    pub(crate) fs_selector: u8,
    /// `SBHUFFDS`: selector for section 6.4.8 subsequent-symbol `S` deltas.
    pub(crate) ds_selector: u8,
    /// `SBHUFFDT`: selector for section 6.4.6 strip `T` deltas.
    pub(crate) dt_selector: u8,
    /// `SBHUFFRDW`: refinement width selector from section 7.4.3.1.2.
    pub(crate) rdw_selector: u8,
    /// `SBHUFFRDH`: refinement height selector from section 7.4.3.1.2.
    pub(crate) rdh_selector: u8,
    /// `SBHUFFRDX`: refinement x selector from section 7.4.3.1.2.
    pub(crate) rdx_selector: u8,
    /// `SBHUFFRDY`: refinement y selector from section 7.4.3.1.2.
    pub(crate) rdy_selector: u8,
    /// `SBHUFFRSIZE`: custom refinement-size table selector from section 7.4.3.1.2.
    pub(crate) rsize_custom: bool,
}

impl TryFrom<u16> for TextRegionHuffmanFlags {
    type Error = Jbig2Error;

    /// Parse text-region Huffman flags from their raw segment-header value.
    ///
    /// ITU-T T.88 | ISO/IEC 14492 section 7.4.3.1.2 defines the bit layout.
    fn try_from(raw_flags: u16) -> Result<Self, Self::Error> {
        let bits = TextRegionHuffmanFlagBits::from_bits_retain(raw_flags);
        Ok(TextRegionHuffmanFlags {
            fs_selector: bits.sbhufffs(),
            ds_selector: bits.sbhuffds(),
            dt_selector: bits.sbhuffdt(),
            rdw_selector: bits.sbhuffrdw(),
            rdh_selector: bits.sbhuffrdh(),
            rdx_selector: bits.sbhuffrdx(),
            rdy_selector: bits.sbhuffrdy(),
            rsize_custom: bits.rsize_custom(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::TextRegionHuffmanFlagBits;

    #[test]
    fn extracts_text_region_huffman_flags() {
        let bits = TextRegionHuffmanFlagBits::from_bits_retain(
            3 | (2 << 2) | (1 << 4) | (1 << 8) | (2 << 10) | (3 << 12) | (1 << 14) | (1 << 15),
        );
        assert_eq!(bits.sbhufffs(), 3);
        assert_eq!(bits.sbhuffds(), 2);
        assert_eq!(bits.sbhuffdt(), 1);
        assert_eq!(bits.sbhuffrdw(), 0);
        assert_eq!(bits.sbhuffrdh(), 1);
        assert_eq!(bits.sbhuffrdx(), 2);
        assert_eq!(bits.sbhuffrdy(), 3);
        assert!(bits.rsize_custom());
        assert!(bits.contains(TextRegionHuffmanFlagBits::RESERVED_15));
    }
}
