use bitflags::bitflags;

pub(crate) const CONTEXT_COUNT: usize = 65_536;
/// `SLTP` context for `GBTEMPLATE = 0`, from section 6.2.5.5.
pub(crate) const TEMPLATE0_TPGD_CONTEXT: usize = 0x9b25;
/// `SLTP` context for `GBTEMPLATE = 1`, from section 6.2.5.5.
pub(super) const TEMPLATE1_TPGD_CONTEXT: usize = 0x0795;
/// `SLTP` context for `GBTEMPLATE = 2` and the supported template-3 path.
pub(crate) const TEMPLATE2_TPGD_CONTEXT: usize = 0x00e5;
/// Packed output bit positions decoded from most significant to least significant.
pub(super) const OPT3_BITS_DESCENDING: [u32; 8] = [7, 6, 5, 4, 3, 2, 1, 0];

bitflags! {
    /// Template-0 optimized generic-region context slots.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub(super) struct Template0Opt3ContextBits: u16 {
        const X1 = 1 << 0;
        const X2 = 1 << 1;
        const X3 = 1 << 2;
        const X4 = 1 << 3;
        const A1 = 1 << 4;
        const X5 = 1 << 5;
        const X6 = 1 << 6;
        const X7 = 1 << 7;
        const X8 = 1 << 8;
        const X9 = 1 << 9;
        const A2 = 1 << 10;
        const A3 = 1 << 11;
        const X10 = 1 << 12;
        const X11 = 1 << 13;
        const X12 = 1 << 14;
        const A4 = 1 << 15;
    }
}

bitflags! {
    /// Template-2 optimized generic-region context slots.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub(super) struct Template2Opt3ContextBits: u16 {
        const X1 = 1 << 0;
        const X2 = 1 << 1;
        const A1 = 1 << 2;
        const X3 = 1 << 3;
        const X4 = 1 << 4;
        const X5 = 1 << 5;
        const X6 = 1 << 6;
        const X7 = 1 << 7;
        const X8 = 1 << 8;
        const X9 = 1 << 9;
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Opt3TemplateConfig {
    /// Context index used for line typical prediction from section 6.2.5.5.
    pub(crate) tpgd_context: usize,
    /// Shift applied when seeding the first previous-row reference byte.
    pub(crate) line1_shift: u32,
    /// Mask selecting first previous-row bits for the initial context.
    pub(crate) context_mask_line1: u32,
    /// Shift applied to the immediately previous row before masking.
    pub(crate) context_right_shift: u32,
    /// Mask selecting immediately previous-row bits for the initial context.
    pub(crate) context_mask_line2: u32,
    /// Mask preserving reusable context bits between adjacent pixels.
    pub(crate) update_context_mask: u32,
    /// Mask selecting the first previous-row bit inserted during context update.
    pub(crate) update_line1_mask: u32,
    /// Mask selecting the immediately previous-row bit inserted during context update.
    pub(crate) update_line2_mask: u32,
}

impl Opt3TemplateConfig {
    /// Optimized configuration for template 0 with default `GBAT` offsets.
    pub(super) const TEMPLATE0: Self = Self {
        tpgd_context: TEMPLATE0_TPGD_CONTEXT,
        line1_shift: 6,
        context_mask_line1: 0xf800,
        context_right_shift: 0,
        context_mask_line2: 0x07f0,
        update_context_mask: 0x7bf7,
        update_line1_mask: 0x0800,
        update_line2_mask: 0x0010,
    };

    /// Optimized configuration for template 2 with default first `GBAT` pair.
    pub(super) const TEMPLATE2: Self = Self {
        tpgd_context: TEMPLATE2_TPGD_CONTEXT,
        line1_shift: 1,
        context_mask_line1: 0x0380,
        context_right_shift: 1,
        context_mask_line2: 0x007c,
        update_context_mask: 0x01bd,
        update_line1_mask: 0x0080,
        update_line2_mask: 0x0004,
    };
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Template12Config {
    /// Bit shift for the single adaptive-template pixel.
    pub(crate) at_shift: u16,
    /// Bit shift for the row two lines above the current pixel.
    pub(crate) line1_shift: u16,
    /// Rolling mask for the row two lines above the current pixel.
    pub(crate) line1_mask: u16,
    /// Rolling mask for the row immediately above the current pixel.
    pub(crate) line2_mask: u16,
    /// Rolling mask for already decoded pixels in the current row.
    pub(crate) line3_mask: u16,
    /// Context index used for line typical prediction from section 6.2.5.5.
    pub(crate) tpgd_context: usize,
    /// Initial x-offsets sampled from the row two lines above.
    pub(crate) line1_seed_x: (i8, i8, Option<i8>),
    /// Initial x-offsets sampled from the row immediately above.
    pub(crate) line2_seed_x: (i8, i8, Option<i8>),
    /// X-offset inserted from the row two lines above as the window advances.
    pub(crate) line1_update_x: i8,
    /// X-offset inserted from the row immediately above as the window advances.
    pub(crate) line2_update_x: i8,
}

impl Template12Config {
    /// Template-1 context geometry from section 6.2.5.3.
    pub(crate) const TEMPLATE1: Self = Self {
        at_shift: 3,
        line1_shift: 9,
        line1_mask: 0x0f,
        line2_mask: 0x1f,
        line3_mask: 0x07,
        tpgd_context: TEMPLATE1_TPGD_CONTEXT,
        line1_seed_x: (2, 1, Some(0)),
        line2_seed_x: (2, 1, Some(0)),
        line1_update_x: 3,
        line2_update_x: 3,
    };

    /// Template-2 context geometry from section 6.2.5.3.
    pub(crate) const TEMPLATE2: Self = Self {
        at_shift: 2,
        line1_shift: 7,
        line1_mask: 0x07,
        line2_mask: 0x0f,
        line3_mask: 0x03,
        tpgd_context: TEMPLATE2_TPGD_CONTEXT,
        line1_seed_x: (1, 0, None),
        line2_seed_x: (1, 0, None),
        line1_update_x: 2,
        line2_update_x: 2,
    };
}

#[cfg(test)]
mod tests {
    use super::{Opt3TemplateConfig, Template0Opt3ContextBits, Template2Opt3ContextBits};

    #[test]
    fn template0_named_masks_match_legacy_values() {
        assert_eq!(Opt3TemplateConfig::TEMPLATE0.context_mask_line1, 0xf800);
        assert_eq!(Opt3TemplateConfig::TEMPLATE0.context_mask_line2, 0x07f0);
        assert_eq!(Opt3TemplateConfig::TEMPLATE0.update_context_mask, 0x7bf7);
        assert_eq!(Opt3TemplateConfig::TEMPLATE0.update_line1_mask, 0x0800);
        assert_eq!(Opt3TemplateConfig::TEMPLATE0.update_line2_mask, 0x0010);
    }

    #[test]
    fn template0_named_masks_capture_initial_reused_and_inserted_roles() {
        let initial_mask = Template0Opt3ContextBits::X10
            .union(Template0Opt3ContextBits::A3)
            .union(Template0Opt3ContextBits::X11)
            .union(Template0Opt3ContextBits::X12)
            .union(Template0Opt3ContextBits::A4)
            .union(Template0Opt3ContextBits::A1)
            .union(Template0Opt3ContextBits::X5)
            .union(Template0Opt3ContextBits::X6)
            .union(Template0Opt3ContextBits::X7)
            .union(Template0Opt3ContextBits::X8)
            .union(Template0Opt3ContextBits::X9)
            .union(Template0Opt3ContextBits::A2);
        let reused_mask = Template0Opt3ContextBits::from_bits_retain(
            u16::try_from(Opt3TemplateConfig::TEMPLATE0.update_context_mask).unwrap_or_default(),
        );
        let inserted_mask = Template0Opt3ContextBits::from_bits_retain(
            u16::try_from(
                Opt3TemplateConfig::TEMPLATE0.update_line1_mask
                    | Opt3TemplateConfig::TEMPLATE0.update_line2_mask,
            )
            .unwrap_or_default(),
        );

        assert_eq!(initial_mask.bits(), 0xfff0);
        assert!(reused_mask.contains(Template0Opt3ContextBits::X1));
        assert!(!reused_mask.contains(Template0Opt3ContextBits::X4));
        assert!(reused_mask.contains(Template0Opt3ContextBits::X9));
        assert!(!reused_mask.contains(Template0Opt3ContextBits::A2));
        assert!(reused_mask.contains(Template0Opt3ContextBits::A3));
        assert!(!reused_mask.contains(Template0Opt3ContextBits::A4));
        assert_eq!(
            inserted_mask,
            Template0Opt3ContextBits::A3.union(Template0Opt3ContextBits::A1)
        );
    }

    #[test]
    fn template2_named_masks_match_legacy_values() {
        assert_eq!(Opt3TemplateConfig::TEMPLATE2.context_mask_line1, 0x0380);
        assert_eq!(Opt3TemplateConfig::TEMPLATE2.context_mask_line2, 0x007c);
        assert_eq!(Opt3TemplateConfig::TEMPLATE2.update_context_mask, 0x01bd);
        assert_eq!(Opt3TemplateConfig::TEMPLATE2.update_line1_mask, 0x0080);
        assert_eq!(Opt3TemplateConfig::TEMPLATE2.update_line2_mask, 0x0004);
    }

    #[test]
    fn template2_named_masks_capture_initial_reused_and_inserted_roles() {
        let initial_mask = Template2Opt3ContextBits::X7
            .union(Template2Opt3ContextBits::X8)
            .union(Template2Opt3ContextBits::X9)
            .union(Template2Opt3ContextBits::A1)
            .union(Template2Opt3ContextBits::X3)
            .union(Template2Opt3ContextBits::X4)
            .union(Template2Opt3ContextBits::X5)
            .union(Template2Opt3ContextBits::X6);
        let reused_mask = Template2Opt3ContextBits::from_bits_retain(
            u16::try_from(Opt3TemplateConfig::TEMPLATE2.update_context_mask).unwrap_or_default(),
        );
        let inserted_mask = Template2Opt3ContextBits::from_bits_retain(
            u16::try_from(
                Opt3TemplateConfig::TEMPLATE2.update_line1_mask
                    | Opt3TemplateConfig::TEMPLATE2.update_line2_mask,
            )
            .unwrap_or_default(),
        );

        assert_eq!(initial_mask.bits(), 0x03fc);
        assert!(reused_mask.contains(Template2Opt3ContextBits::X1));
        assert!(!reused_mask.contains(Template2Opt3ContextBits::X2));
        assert!(!reused_mask.contains(Template2Opt3ContextBits::X6));
        assert!(!reused_mask.contains(Template2Opt3ContextBits::X9));
        assert_eq!(
            inserted_mask,
            Template2Opt3ContextBits::X7.union(Template2Opt3ContextBits::A1)
        );
    }
}
