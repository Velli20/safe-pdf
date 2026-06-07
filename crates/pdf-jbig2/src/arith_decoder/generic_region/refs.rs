//! Template-reference adapters for the generic-region row loop.
//!
//! The concrete reference-window structs live in `template_refs`; this module
//! gives the unoptimized section 6.2.5.7 row loop a single trait for template
//! 0, templates 1/2, and template 3.

use crate::{
    arith_decoder::template_refs::{Template0Refs, Template3Refs, Template12Refs},
    error::Jbig2Error,
    generic_region::{GenericRegionAdaptiveTemplate, tables::Template12Config},
    image::JBig2Image,
};

/// Generic-region template reference behavior needed by the pixel loop.
///
/// ITU-T T.88 / ISO/IEC 14492 section 6.2.5.3 defines different context
/// shapes for each `GBTEMPLATE`, but section 6.2.5.7 advances every template
/// in the same row-major order.
pub(super) trait GenericTemplateRefs {
    /// Template-specific configuration used while building contexts.
    type Config: Copy;

    /// Seed reference state for the start of `row`.
    fn new(image: &JBig2Image, row: u16, config: Self::Config) -> Self;

    /// Build the arithmetic context for the current generic-region pixel.
    fn context(
        &self,
        image: &JBig2Image,
        col: u16,
        row: u16,
        gbat: &GenericRegionAdaptiveTemplate,
        config: Self::Config,
    ) -> Result<usize, Jbig2Error>;

    /// Advance reference state after one pixel is output.
    fn advance(&mut self, image: &JBig2Image, col: u16, row: u16, pixel: u8, config: Self::Config);
}

/// Adapter for the template-0 reference window from section 6.2.5.3.
pub(super) struct Template0DecodeRefs(Template0Refs);

impl GenericTemplateRefs for Template0DecodeRefs {
    type Config = ();

    fn new(image: &JBig2Image, row: u16, _config: Self::Config) -> Self {
        Self(Template0Refs::new(image, row))
    }

    fn context(
        &self,
        image: &JBig2Image,
        col: u16,
        row: u16,
        gbat: &GenericRegionAdaptiveTemplate,
        _config: Self::Config,
    ) -> Result<usize, Jbig2Error> {
        self.0.context(image, col, row, gbat)
    }

    fn advance(
        &mut self,
        image: &JBig2Image,
        col: u16,
        row: u16,
        pixel: u8,
        _config: Self::Config,
    ) {
        self.0.advance(image, col, row, pixel);
    }
}

/// Adapter for the shared template-1/template-2 reference window.
pub(super) struct Template12DecodeRefs(Template12Refs);

impl GenericTemplateRefs for Template12DecodeRefs {
    type Config = Template12Config;

    fn new(image: &JBig2Image, row: u16, config: Self::Config) -> Self {
        Self(Template12Refs::new(image, row, config))
    }

    fn context(
        &self,
        image: &JBig2Image,
        col: u16,
        row: u16,
        gbat: &GenericRegionAdaptiveTemplate,
        config: Self::Config,
    ) -> Result<usize, Jbig2Error> {
        self.0.context(image, col, row, gbat, config)
    }

    fn advance(&mut self, image: &JBig2Image, col: u16, row: u16, pixel: u8, config: Self::Config) {
        self.0.advance(image, col, row, pixel, config);
    }
}

/// Adapter for the template-3 reference window from section 6.2.5.3.
pub(super) struct Template3DecodeRefs(Template3Refs);

impl GenericTemplateRefs for Template3DecodeRefs {
    type Config = ();

    fn new(image: &JBig2Image, row: u16, _config: Self::Config) -> Self {
        Self(Template3Refs::new(image, row))
    }

    fn context(
        &self,
        image: &JBig2Image,
        col: u16,
        row: u16,
        gbat: &GenericRegionAdaptiveTemplate,
        _config: Self::Config,
    ) -> Result<usize, Jbig2Error> {
        self.0.context(image, col, row, gbat)
    }

    fn advance(
        &mut self,
        image: &JBig2Image,
        col: u16,
        row: u16,
        pixel: u8,
        _config: Self::Config,
    ) {
        self.0.advance(image, col, row, pixel);
    }
}
