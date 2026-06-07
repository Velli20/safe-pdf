//! Pixel-oriented arithmetic generic-region decode loop.
//!
//! ITU-T T.88 / ISO/IEC 14492 section 6.2.5.7 defines one row-major loop for
//! arithmetic generic regions. This module owns that loop; template-specific
//! context construction is supplied by `GenericTemplateRefs`.

use crate::{
    arith_decoder::JBig2ArithDecoder,
    error::Jbig2Error,
    generic_region::{GenericRegionAdaptiveTemplate, tables::Template12Config},
    image::JBig2Image,
};

use super::{refs::GenericTemplateRefs, skip::SkipBitmap};

/// Fully specified unoptimized generic-template decode configuration.
///
/// The fields correspond to the inputs used by ITU-T T.88 / ISO/IEC 14492
/// section 6.2.5.7: bitmap dimensions, optional typical prediction from
/// section 6.2.5.5, a template-specific `SLTP` context, and template-reference
/// state configuration from section 6.2.5.3.
#[derive(Clone, Copy)]
pub(super) struct GenericTemplateDecodeConfig<T> {
    width: u16,
    height: u16,
    tpgdon: bool,
    tpgd_context: usize,
    refs_config: T,
}

impl GenericTemplateDecodeConfig<()> {
    /// Build a decode configuration for `GBTEMPLATE = 0`.
    pub(super) const fn template0(width: u16, height: u16, tpgdon: bool) -> Self {
        Self {
            width,
            height,
            tpgdon,
            tpgd_context: super::template0_tpgd_context(),
            refs_config: (),
        }
    }

    /// Build a decode configuration for `GBTEMPLATE = 3`.
    pub(super) const fn template3(width: u16, height: u16, tpgdon: bool) -> Self {
        Self {
            width,
            height,
            tpgdon,
            tpgd_context: super::template3_tpgd_context(),
            refs_config: (),
        }
    }
}

impl GenericTemplateDecodeConfig<Template12Config> {
    /// Build a decode configuration for `GBTEMPLATE = 1` or `GBTEMPLATE = 2`.
    pub(super) const fn template12(
        width: u16,
        height: u16,
        tpgdon: bool,
        config: Template12Config,
    ) -> Self {
        Self {
            width,
            height,
            tpgdon,
            tpgd_context: config.tpgd_context,
            refs_config: config,
        }
    }
}

/// Decode an arithmetic generic region using the supplied template references.
///
/// This is the section 6.2.5.7 row loop: optional `LTP` update, optional line
/// copy for typical prediction, left-to-right pixel decoding, and skipped
/// pixels forced to zero without arithmetic consumption.
pub(super) fn decode_template_with_refs<T>(
    decoder: &mut JBig2ArithDecoder<'_, '_>,
    decode_config: GenericTemplateDecodeConfig<T::Config>,
    gbat: &GenericRegionAdaptiveTemplate,
    skip: Option<&JBig2Image>,
) -> Result<JBig2Image, Jbig2Error>
where
    T: GenericTemplateRefs,
{
    let width = decode_config.width;
    let height = decode_config.height;
    let refs_config = decode_config.refs_config;
    let skip = SkipBitmap::new(skip);
    let mut image = JBig2Image::try_new(width, height, Some(false))?;
    let mut ltp = 0u8;
    decoder.ensure_generic_region_contexts()?;

    for row in 0..height {
        if decode_config.tpgdon {
            ltp ^= decoder.decode_prepared_generic_context(decode_config.tpgd_context)?;
        }
        if ltp != 0 {
            image.copy_line(row, row.saturating_sub(1));
            continue;
        }

        decode_row::<T>(decoder, &mut image, row, refs_config, gbat, skip)?;
    }

    Ok(image)
}

/// Decode one generic-region row with the selected template references.
///
/// Section 6.2.5.7 decodes pixels in increasing column order. This helper
/// keeps reference-window advancement next to each decoded pixel so tests can
/// exercise row behavior independently from template dispatch.
fn decode_row<T>(
    decoder: &mut JBig2ArithDecoder<'_, '_>,
    image: &mut JBig2Image,
    row: u16,
    refs_config: T::Config,
    gbat: &GenericRegionAdaptiveTemplate,
    skip: SkipBitmap<'_>,
) -> Result<(), Jbig2Error>
where
    T: GenericTemplateRefs,
{
    let mut refs = T::new(image, row, refs_config);
    for col in 0..image.width() {
        let pixel = decode_pixel::<T>(
            decoder,
            image,
            &refs,
            PixelDecodeParams {
                col,
                row,
                refs_config,
                gbat,
                skip,
            },
        )?;
        refs.advance(image, col, row, pixel, refs_config);
    }
    Ok(())
}

/// Inputs needed to decode one unoptimized generic-region pixel.
///
/// The values are the section 6.2.5.7 raster position, template-reference
/// configuration from section 6.2.5.3, adaptive-template offsets from section
/// 6.2.5.4, and optional skip policy from section 6.2.5.6.
#[derive(Clone, Copy)]
struct PixelDecodeParams<'a, C> {
    col: u16,
    row: u16,
    refs_config: C,
    gbat: &'a GenericRegionAdaptiveTemplate,
    skip: SkipBitmap<'a>,
}

/// Decode one generic-region pixel or apply the skip-bitmap zero rule.
///
/// ITU-T T.88 / ISO/IEC 14492 section 6.2.5.6 specifies that skipped pixels
/// are set to zero and do not consume an arithmetic-coded decision.
fn decode_pixel<T>(
    decoder: &mut JBig2ArithDecoder<'_, '_>,
    image: &mut JBig2Image,
    refs: &T,
    params: PixelDecodeParams<'_, T::Config>,
) -> Result<u8, Jbig2Error>
where
    T: GenericTemplateRefs,
{
    if params.skip.is_skipped(params.col, params.row) {
        return Ok(0);
    }

    let context = refs.context(
        image,
        params.col,
        params.row,
        params.gbat,
        params.refs_config,
    )?;
    let pixel = decoder.decode_prepared_generic_context(context)?;
    image.set_pixel_if_black(params.col, params.row, pixel);
    Ok(pixel)
}
