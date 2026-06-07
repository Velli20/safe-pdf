//! Rolling context-reference state for JBIG2 arithmetic generic-region templates.
//!
//! ITU-T T.88 / ISO/IEC 14492 section 6.2.5.3 defines the generic-region
//! template shapes, section 6.2.5.4 defines adaptive template pixels (`GBAT`),
//! and section 6.2.5.7 defines the row-major arithmetic pixel loop that
//! advances these references. The ITU publication is available at
//! <https://www.itu.int/rec/T-REC-T.88>.
//!
//! This module isolates the unoptimized reference-window bookkeeping used by
//! the generic-region decoder; the bit placements mirror the specification's
//! template figures and the reference implementation structure in jbig2dec's
//! `jbig2_generic.c`.

use crate::{
    error::Jbig2Error,
    generic_region::{
        GenericRegionAdaptiveTemplate,
        tables::{Opt3TemplateConfig, Template12Config},
    },
    image::JBig2Image,
};

/// Byte-oriented rolling reference state for optimized generic-region templates.
///
/// The optimized decoder consumes and updates reference rows a byte at a time,
/// while preserving the same template context geometry as the pixel-oriented
/// template reference structs below.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Opt3Refs {
    line1: u32,
    line2: u32,
    line1_row: u16,
    line2_row: u16,
    p1: usize,
    p2: usize,
    use_line2: bool,
    tail_padded: bool,
}

impl Opt3Refs {
    /// Seed optimized references from the two preceding rows.
    pub(crate) fn with_two_refs(
        image: &JBig2Image,
        row: u16,
        config: Opt3TemplateConfig,
    ) -> Result<Self, Jbig2Error> {
        let line1_row = row
            .checked_sub(2)
            .ok_or(Jbig2Error::InvalidState("arithmetic row reference"))?;
        let line2_row = row
            .checked_sub(1)
            .ok_or(Jbig2Error::InvalidState("arithmetic row reference"))?;

        Ok(Self {
            line1: u32::from(image.read_row_byte(line1_row, 0)?) << config.line1_shift,
            line2: u32::from(image.read_row_byte(line2_row, 0)?),
            line1_row,
            line2_row,
            p1: 1,
            p2: 1,
            use_line2: true,
            tail_padded: false,
        })
    }

    /// Seed optimized references when only the immediately preceding row is active.
    pub(crate) fn with_one_ref(image: &JBig2Image, row: u16) -> Result<Self, Jbig2Error> {
        let line2_row = row.saturating_sub(1);
        let use_line2 = row & 1 == 1;
        let (line2, p2) = if use_line2 {
            (u32::from(image.read_row_byte(line2_row, 0)?), 1)
        } else {
            (0, 0)
        };

        Ok(Self {
            line1: 0,
            line2,
            line1_row: 0,
            line2_row,
            p1: 0,
            p2,
            use_line2,
            tail_padded: false,
        })
    }

    /// Build the current optimized context when two previous rows are active.
    pub(crate) fn context_with_two_refs(self, config: Opt3TemplateConfig) -> u32 {
        (self.line1 & config.context_mask_line1)
            | ((self.line2 >> config.context_right_shift) & config.context_mask_line2)
    }

    /// Build the current optimized context when only one previous row is active.
    pub(crate) fn context_with_one_ref(self, config: Opt3TemplateConfig) -> u32 {
        (self.line2 >> config.context_right_shift) & config.context_mask_line2
    }

    /// Advance optimized references after a decoded pixel in two-reference mode.
    pub(crate) fn advance_with_two_refs(
        &mut self,
        image: &JBig2Image,
        bit: Option<u32>,
        config: Opt3TemplateConfig,
    ) -> Result<u32, Jbig2Error> {
        let Some(bit) = bit else {
            self.pad_tail();
            return Ok(0);
        };
        if bit == 7 && !self.tail_padded {
            self.line1 = (self.line1 << 8)
                | (u32::from(image.read_row_byte(self.line1_row, self.p1)?) << config.line1_shift);
            self.p1 = self.p1.saturating_add(1);
            self.line2 =
                (self.line2 << 8) | u32::from(image.read_row_byte(self.line2_row, self.p2)?);
            self.p2 = self.p2.saturating_add(1);
        }

        self.update_context_bits(bit, config)
    }

    /// Advance optimized references after a decoded pixel in one-reference mode.
    pub(crate) fn advance_with_one_ref(
        &mut self,
        image: &JBig2Image,
        bit: Option<u32>,
        config: Opt3TemplateConfig,
    ) -> Result<u32, Jbig2Error> {
        let Some(bit) = bit else {
            self.pad_tail();
            return Ok(0);
        };
        if bit == 7 && self.use_line2 && !self.tail_padded {
            self.line2 =
                (self.line2 << 8) | u32::from(image.read_row_byte(self.line2_row, self.p2)?);
            self.p2 = self.p2.saturating_add(1);
        }

        let line2_shift = bit
            .checked_add(config.context_right_shift)
            .ok_or(Jbig2Error::InvalidState("bit shift"))?;
        Ok((self.line2 >> line2_shift) & config.update_line2_mask)
    }

    fn update_context_bits(self, bit: u32, config: Opt3TemplateConfig) -> Result<u32, Jbig2Error> {
        let line2_shift = bit
            .checked_add(config.context_right_shift)
            .ok_or(Jbig2Error::InvalidState("bit shift"))?;
        Ok(((self.line1 >> bit) & config.update_line1_mask)
            | ((self.line2 >> line2_shift) & config.update_line2_mask))
    }

    fn pad_tail(&mut self) {
        self.line1 <<= 8;
        self.line2 <<= 8;
        self.tail_padded = true;
    }
}

/// Rolling reference-window state for arithmetic generic-region template 0.
///
/// `GBTEMPLATE = 0` uses the 16-pixel context from ITU-T T.88 / ISO/IEC 14492
/// section 6.2.5.3, with up to four adaptive pixels from section 6.2.5.4.
/// The stored lines hold the fixed, previously decoded pixels that can be
/// shifted forward as the section 6.2.5.7 loop advances across a row.
pub(super) struct Template0Refs {
    line1: u16,
    line2: u16,
    line3: u16,
}

impl Template0Refs {
    /// Seed the template-0 reference window for the start of `row`.
    pub(super) fn new(image: &JBig2Image, row: u16) -> Self {
        let mut line1 = image.pixel_at_offset(1, row, 0, -2);
        line1 |= image.pixel_at_offset(0, row, 0, -2) << 1;
        let mut line2 = image.pixel_at_offset(2, row, 0, -1);
        line2 |= image.pixel_at_offset(1, row, 0, -1) << 1;
        line2 |= image.pixel_at_offset(0, row, 0, -1) << 2;

        Self {
            line1,
            line2,
            line3: 0,
        }
    }

    /// Build the arithmetic context for the current template-0 pixel.
    ///
    /// Fixed reference pixels come from the rolling window, and adaptive
    /// pixels are read from the normalized `GBAT` offsets defined by
    /// ITU-T T.88 / ISO/IEC 14492 section 6.2.5.4.
    pub(super) fn context(
        &self,
        image: &JBig2Image,
        col: u16,
        row: u16,
        gbat: &GenericRegionAdaptiveTemplate,
    ) -> Result<usize, Jbig2Error> {
        let mut context = self.line3;
        context |= image.pixel_at_gbat_offset(col, row, *gbat, 0)? << 4;
        context |= self.line2 << 5;
        context |= image.pixel_at_gbat_offset(col, row, *gbat, 2)? << 10;
        context |= image.pixel_at_gbat_offset(col, row, *gbat, 4)? << 11;
        context |= self.line1 << 12;
        context |= image.pixel_at_gbat_offset(col, row, *gbat, 6)? << 15;
        Ok(usize::from(context))
    }

    /// Advance the template-0 reference window after decoding one pixel.
    pub(super) fn advance(&mut self, image: &JBig2Image, col: u16, row: u16, pixel: u8) {
        self.line1 = ((self.line1 << 1) | image.pixel_at_offset(col, row, 2, -2)) & 0x07;
        self.line2 = ((self.line2 << 1) | image.pixel_at_offset(col, row, 3, -1)) & 0x1f;
        self.line3 = ((self.line3 << 1) | u16::from(pixel)) & 0x0f;
    }
}

/// Rolling reference-window state shared by arithmetic generic-region templates 1 and 2.
///
/// ITU-T T.88 / ISO/IEC 14492 section 6.2.5.3 gives `GBTEMPLATE = 1` and
/// `GBTEMPLATE = 2` distinct context shapes, but both use the same row-major
/// update model from section 6.2.5.7 and one adaptive pixel from section
/// 6.2.5.4. `Template12Config` supplies the template-specific bit placement.
pub(super) struct Template12Refs {
    line1: u16,
    line2: u16,
    line3: u16,
}

impl Template12Refs {
    /// Seed the template-1 or template-2 reference window for the start of `row`.
    pub(super) fn new(image: &JBig2Image, row: u16, config: Template12Config) -> Self {
        let (line1_x0, line1_x1, line1_x2) = config.line1_seed_x;
        let mut line1 = image.pixel_at_offset(0, row, line1_x0, -2);
        line1 |= image.pixel_at_offset(0, row, line1_x1, -2) << 1;
        if let Some(x) = line1_x2 {
            line1 |= image.pixel_at_offset(0, row, x, -2) << 2;
        }

        let (line2_x0, line2_x1, line2_x2) = config.line2_seed_x;
        let mut line2 = image.pixel_at_offset(0, row, line2_x0, -1);
        line2 |= image.pixel_at_offset(0, row, line2_x1, -1) << 1;
        if let Some(x) = line2_x2 {
            line2 |= image.pixel_at_offset(0, row, x, -1) << 2;
        }

        Self {
            line1,
            line2,
            line3: 0,
        }
    }

    /// Build the arithmetic context for the current template-1 or template-2 pixel.
    ///
    /// `config` selects the bit layout from the corresponding
    /// ITU-T T.88 / ISO/IEC 14492 section 6.2.5.3 template figure.
    pub(super) fn context(
        &self,
        image: &JBig2Image,
        col: u16,
        row: u16,
        gbat: &GenericRegionAdaptiveTemplate,
        config: Template12Config,
    ) -> Result<usize, Jbig2Error> {
        let mut context = self.line3;
        context |= image.pixel_at_gbat_offset(col, row, *gbat, 0)? << config.at_shift;
        let line2_shift = config
            .at_shift
            .checked_add(1)
            .ok_or(Jbig2Error::InvalidState("bit shift"))?;
        context |= self.line2 << line2_shift;
        context |= self.line1 << config.line1_shift;
        Ok(usize::from(context))
    }

    /// Advance the template-1 or template-2 reference window after decoding one pixel.
    pub(super) fn advance(
        &mut self,
        image: &JBig2Image,
        col: u16,
        row: u16,
        pixel: u8,
        config: Template12Config,
    ) {
        self.line1 = ((self.line1 << 1)
            | image.pixel_at_offset(col, row, config.line1_update_x, -2))
            & config.line1_mask;
        self.line2 = ((self.line2 << 1)
            | image.pixel_at_offset(col, row, config.line2_update_x, -1))
            & config.line2_mask;
        self.line3 = ((self.line3 << 1) | u16::from(pixel)) & config.line3_mask;
    }
}

/// Rolling reference-window state for arithmetic generic-region template 3.
///
/// `GBTEMPLATE = 3` uses the smallest generic-region context from
/// ITU-T T.88 / ISO/IEC 14492 section 6.2.5.3: previously decoded pixels
/// from the current row, fixed pixels from the previous row, and one adaptive
/// pixel from section 6.2.5.4.
pub(super) struct Template3Refs {
    line1: u16,
    line2: u16,
}

impl Template3Refs {
    /// Seed the template-3 reference window for the start of `row`.
    pub(super) fn new(image: &JBig2Image, row: u16) -> Self {
        let mut line1 = image.pixel_at_offset(1, row, 0, -1);
        line1 |= image.pixel_at_offset(0, row, 0, -1) << 1;
        Self { line1, line2: 0 }
    }

    /// Build the arithmetic context for the current template-3 pixel.
    ///
    /// The context combines the rolling current-row and previous-row windows
    /// with the normalized single adaptive `GBAT` offset for template 3.
    pub(super) fn context(
        &self,
        image: &JBig2Image,
        col: u16,
        row: u16,
        gbat: &GenericRegionAdaptiveTemplate,
    ) -> Result<usize, Jbig2Error> {
        let mut context = self.line2;
        context |= image.pixel_at_gbat_offset(col, row, *gbat, 0)? << 4;
        context |= self.line1 << 5;
        Ok(usize::from(context))
    }

    /// Advance the template-3 reference window after decoding one pixel.
    pub(super) fn advance(&mut self, image: &JBig2Image, col: u16, row: u16, pixel: u8) {
        self.line1 = ((self.line1 << 1) | image.pixel_at_offset(col, row, 2, -1)) & 0x1f;
        self.line2 = ((self.line2 << 1) | u16::from(pixel)) & 0x0f;
    }
}
