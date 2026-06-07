//! Arithmetic decoding entrypoints for JBIG2 generic regions.
//!
//! ITU-T T.88 / ISO/IEC 14492 section 6.2.5.7 defines the arithmetic generic-
//! region bitmap loop. This module dispatches the four `GBTEMPLATE` layouts
//! from section 6.2.5.3 to small decode helpers while preserving the existing
//! `JBig2ArithDecoder` method surface used by higher-level JBIG2 code.

mod refs;
mod skip;
mod unoptimized;

use crate::{
    arith_decoder::JBig2ArithDecoder,
    error::Jbig2Error,
    generic_region::{
        GenericRegionAdaptiveTemplate, GenericRegionTemplate,
        tables::{TEMPLATE0_TPGD_CONTEXT, TEMPLATE2_TPGD_CONTEXT, Template12Config},
    },
    image::JBig2Image,
};

use self::{
    refs::{Template0DecodeRefs, Template3DecodeRefs, Template12DecodeRefs},
    unoptimized::GenericTemplateDecodeConfig,
};

impl JBig2ArithDecoder<'_, '_> {
    /// Decode a JBIG2 arithmetic generic region with the unoptimized template path.
    ///
    /// This implements the bitmap loop from ITU-T T.88 / ISO/IEC 14492
    /// section 6.2.5.7 for `GBTEMPLATE` values 0 through 3. Template geometry
    /// and adaptive pixels are taken from sections 6.2.5.3 and 6.2.5.4;
    /// line typical prediction follows section 6.2.5.5, and skip handling
    /// follows section 6.2.5.6.
    pub(crate) fn decode_arith_template_unopt(
        &mut self,
        width: u16,
        height: u16,
        gbt_template: GenericRegionTemplate,
        tpgdon: bool,
        gbat: &GenericRegionAdaptiveTemplate,
        skip: Option<&JBig2Image>,
    ) -> Result<JBig2Image, Jbig2Error> {
        match gbt_template {
            GenericRegionTemplate::Template0 => {
                self.decode_arith_template0_unopt_skip(width, height, tpgdon, gbat, skip)
            }
            GenericRegionTemplate::Template1 => self.decode_arith_template12_unopt_skip(
                width,
                height,
                Template12Config::TEMPLATE1,
                tpgdon,
                gbat,
                skip,
            ),
            GenericRegionTemplate::Template2 => self.decode_arith_template12_unopt_skip(
                width,
                height,
                Template12Config::TEMPLATE2,
                tpgdon,
                gbat,
                skip,
            ),
            GenericRegionTemplate::Template3 => {
                self.decode_arith_template3_unopt_skip(width, height, tpgdon, gbat, skip)
            }
        }
    }

    /// Decode `GBTEMPLATE = 0` without the optimized default-template shortcut.
    ///
    /// Template 0 is the 16-pixel generic-region context described by section
    /// 6.2.5.3, with up to four adaptive template pairs from section 6.2.5.4.
    /// This method runs the section 6.2.5.7 row/pixel loop and applies the
    /// section 6.2.5.6 skip rule when a `SKIP` bitmap is supplied.
    pub(crate) fn decode_arith_template0_unopt_skip(
        &mut self,
        width: u16,
        height: u16,
        tpgdon: bool,
        gbat: &GenericRegionAdaptiveTemplate,
        skip: Option<&JBig2Image>,
    ) -> Result<JBig2Image, Jbig2Error> {
        unoptimized::decode_template_with_refs::<Template0DecodeRefs>(
            self,
            GenericTemplateDecodeConfig::template0(width, height, tpgdon),
            gbat,
            skip,
        )
    }

    /// Decode `GBTEMPLATE = 1` or `GBTEMPLATE = 2` with skipped-pixel support.
    ///
    /// Sections 6.2.5.3 and 6.2.5.4 give templates 1 and 2 different context
    /// geometry but the same decode control flow. `Template12Config` supplies
    /// the template-specific masks, shifts, and typical-prediction context.
    pub(crate) fn decode_arith_template12_unopt_skip(
        &mut self,
        width: u16,
        height: u16,
        config: Template12Config,
        tpgdon: bool,
        gbat: &GenericRegionAdaptiveTemplate,
        skip: Option<&JBig2Image>,
    ) -> Result<JBig2Image, Jbig2Error> {
        unoptimized::decode_template_with_refs::<Template12DecodeRefs>(
            self,
            GenericTemplateDecodeConfig::template12(width, height, tpgdon, config),
            gbat,
            skip,
        )
    }

    /// Decode `GBTEMPLATE = 3` with skipped-pixel support.
    ///
    /// Template 3 is the smallest arithmetic generic-region template in
    /// section 6.2.5.3. It uses one adaptive `GBAT` pair from section 6.2.5.4,
    /// the template-3 typical-prediction context from section 6.2.5.5, and
    /// the skip rule from section 6.2.5.6.
    pub(crate) fn decode_arith_template3_unopt_skip(
        &mut self,
        width: u16,
        height: u16,
        tpgdon: bool,
        gbat: &GenericRegionAdaptiveTemplate,
        skip: Option<&JBig2Image>,
    ) -> Result<JBig2Image, Jbig2Error> {
        unoptimized::decode_template_with_refs::<Template3DecodeRefs>(
            self,
            GenericTemplateDecodeConfig::template3(width, height, tpgdon),
            gbat,
            skip,
        )
    }
}

/// Return the line typical-prediction context for template 0.
pub(super) const fn template0_tpgd_context() -> usize {
    TEMPLATE0_TPGD_CONTEXT
}

/// Return the line typical-prediction context for template 3.
pub(super) const fn template3_tpgd_context() -> usize {
    TEMPLATE2_TPGD_CONTEXT
}
