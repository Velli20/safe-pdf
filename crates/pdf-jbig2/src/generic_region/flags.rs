//! JBIG2 generic-region flag parsing.
//!
//! ITU-T T.88 / ISO/IEC 14492 section 7.4.6.2 defines the single generic-
//! region flags byte. This module names those bit fields and converts them
//! into typed decoder configuration.

use bitflags::bitflags;

use crate::error::Jbig2Error;

use super::GenericRegionAdaptiveTemplate;

const MMR_FLAG_BIT: u8 = 0;
const GB_TEMPLATE_SHIFT: u8 = 1;
const GB_TEMPLATE_WIDTH: u8 = 2;
const TPGDON_FLAG_BIT: u8 = 3;
const GB_TEMPLATE_MASK_BITS: u8 = ((1u8 << GB_TEMPLATE_WIDTH) - 1) << GB_TEMPLATE_SHIFT;

bitflags! {
    /// Raw JBIG2 generic-region flags from section 7.4.6.2.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    struct GenericRegionFlagBits: u8 {
        const MMR = 1 << MMR_FLAG_BIT;
        const GB_TEMPLATE_MASK = GB_TEMPLATE_MASK_BITS;
        const TPGDON = 1 << TPGDON_FLAG_BIT;
    }
}

impl GenericRegionFlagBits {
    /// Return the typed arithmetic template id from the `GBTEMPLATE` bits.
    fn gbt_template(self) -> Result<GenericRegionTemplate, Jbig2Error> {
        GenericRegionTemplate::try_from(
            (self.bits() & Self::GB_TEMPLATE_MASK.bits()) >> GB_TEMPLATE_SHIFT,
        )
    }

    /// Return the raw `GBTEMPLATE` field for tests of section 7.4.6.2 parsing.
    #[cfg(test)]
    fn raw_gbt_template(self) -> u8 {
        (self.bits() & Self::GB_TEMPLATE_MASK.bits()) >> GB_TEMPLATE_SHIFT
    }
}

/// JBIG2 arithmetic generic-region template selector.
///
/// The variants correspond to `GBTEMPLATE` values 0 through 3 in ITU-T T.88 /
/// ISO/IEC 14492 sections 6.2.5.3 and 7.4.6.2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GenericRegionTemplate {
    /// Template 0, the 16-pixel context with four adaptive template pairs.
    Template0,
    /// Template 1, a reduced context with one adaptive template pair.
    Template1,
    /// Template 2, a reduced context with one adaptive template pair.
    Template2,
    /// Template 3, the smallest generic-region arithmetic context.
    Template3,
}

impl TryFrom<u8> for GenericRegionTemplate {
    type Error = Jbig2Error;

    /// Convert a raw `GBTEMPLATE` field into a typed template selector.
    fn try_from(raw_template: u8) -> Result<Self, Self::Error> {
        match raw_template {
            0 => Ok(Self::Template0),
            1 => Ok(Self::Template1),
            2 => Ok(Self::Template2),
            3 => Ok(Self::Template3),
            _ => Err(Jbig2Error::UnsupportedFeature(
                "arithmetic generic region template",
            )),
        }
    }
}

/// Parsed JBIG2 generic-region flags and normalized adaptive-template data.
///
/// Section 7.4.6.2 supplies `MMR`, `GBTEMPLATE`, and `TPGDON`; the associated
/// adaptive template data is normalized into `gbat` after the flags byte is
/// parsed.
#[derive(Debug, Clone, Copy)]
pub(crate) struct GenericRegionFlags {
    /// Whether section 7.4.6.2 selects MMR decoding instead of arithmetic.
    pub(crate) mmr: bool,
    /// Arithmetic generic-region template from sections 6.2.5.3 and 7.4.6.2.
    pub(crate) gbt_template: GenericRegionTemplate,
    /// Whether section 6.2.5.5 typical prediction is enabled.
    pub(crate) tpgdon: bool,
    /// Normalized generic-region adaptive-template data from section 6.2.5.4.
    pub(crate) gbat: GenericRegionAdaptiveTemplate,
}

impl TryFrom<u8> for GenericRegionFlags {
    type Error = Jbig2Error;

    /// Parse a raw section 7.4.6.2 generic-region flags byte.
    fn try_from(raw_flags: u8) -> Result<Self, Self::Error> {
        let bits = GenericRegionFlagBits::from_bits_retain(raw_flags);
        let gbt_template = bits.gbt_template()?;
        Ok(GenericRegionFlags {
            mmr: bits.contains(GenericRegionFlagBits::MMR),
            gbt_template,
            tpgdon: bits.contains(GenericRegionFlagBits::TPGDON),
            gbat: GenericRegionAdaptiveTemplate::from(&[], 0, true, gbt_template)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{GB_TEMPLATE_SHIFT, GenericRegionFlagBits, GenericRegionTemplate};

    #[test]
    fn extracts_generic_region_flags() {
        let bits = GenericRegionFlagBits::from_bits_retain(
            (1 << super::MMR_FLAG_BIT) | (2 << GB_TEMPLATE_SHIFT) | (1 << super::TPGDON_FLAG_BIT),
        );
        assert!(bits.contains(GenericRegionFlagBits::MMR));
        assert_eq!(bits.gbt_template(), Ok(GenericRegionTemplate::Template2));
        assert_eq!(bits.raw_gbt_template(), 2);
        assert!(bits.contains(GenericRegionFlagBits::TPGDON));
    }
}
