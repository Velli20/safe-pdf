//! JBIG2 generic refinement-region decoding.
//!
//! ITU-T T.88 / ISO/IEC 14492 section 7.4.7 defines the generic refinement
//! region segment syntax, and section 6.3.2 defines the arithmetic generic
//! refinement region decoding procedure implemented by this module.

use bitflags::bitflags;

use crate::{
    arith_decoder::JBig2ArithDecoder, compose_op::ComposeOp, error::Jbig2Error, image::JBig2Image,
    region_info::RegionInfo, segment_context::SegmentDecodeContext,
};

/// Error label for the arithmetic-coded refinement bitmap body.
///
/// T.88 / ISO/IEC 14492 section 7.4.7 places the coded bitmap data after the
/// generic refinement region segment header fields.
const GENERIC_REFINEMENT_BODY: &str = "generic refinement region body";

/// Current-region context pixels for refinement template 0.
///
/// T.88 / ISO/IEC 14492 section 6.3.2 defines the current bitmap template
/// pixels that contribute to the generic refinement arithmetic context.
const TEMPLATE0_CODING: [RefinementOffset; 3] = [
    RefinementOffset { x: 0, y: -1 },
    RefinementOffset { x: 1, y: -1 },
    RefinementOffset { x: -1, y: 0 },
];

/// Reference-image context pixels for refinement template 0.
///
/// T.88 / ISO/IEC 14492 section 6.3.2 defines the reference bitmap template
/// pixels that follow the current bitmap pixels in the context label.
const TEMPLATE0_REFERENCE: [RefinementOffset; 8] = [
    RefinementOffset { x: 0, y: -1 },
    RefinementOffset { x: 1, y: -1 },
    RefinementOffset { x: -1, y: 0 },
    RefinementOffset { x: 0, y: 0 },
    RefinementOffset { x: 1, y: 0 },
    RefinementOffset { x: -1, y: 1 },
    RefinementOffset { x: 0, y: 1 },
    RefinementOffset { x: 1, y: 1 },
];

/// Current-region context pixels for refinement template 1.
///
/// T.88 / ISO/IEC 14492 section 6.3.2 defines template 1 as the alternate
/// generic refinement template selected by the segment flags in section 7.4.7.2.
const TEMPLATE1_CODING: [RefinementOffset; 4] = [
    RefinementOffset { x: -1, y: -1 },
    RefinementOffset { x: 0, y: -1 },
    RefinementOffset { x: 1, y: -1 },
    RefinementOffset { x: -1, y: 0 },
];

/// Reference-image context pixels for refinement template 1.
///
/// T.88 / ISO/IEC 14492 section 6.3.2 defines the template 1 reference bitmap
/// pixels used to complete the arithmetic context label.
const TEMPLATE1_REFERENCE: [RefinementOffset; 6] = [
    RefinementOffset { x: 0, y: -1 },
    RefinementOffset { x: -1, y: 0 },
    RefinementOffset { x: 0, y: 0 },
    RefinementOffset { x: 1, y: 0 },
    RefinementOffset { x: 0, y: 1 },
    RefinementOffset { x: 1, y: 1 },
];

bitflags! {
    /// Generic refinement region flags from T.88 section 7.4.7.2.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    struct GenericRefinementRegionFlagBits: u8 {
        /// Selects generic refinement template 0 or 1.
        ///
        /// T.88 / ISO/IEC 14492 section 7.4.7.2 defines this bit as
        /// `GRTEMPLATE`.
        const TEMPLATE = 1 << 0;

        /// Enables line typical prediction for the refinement region.
        ///
        /// T.88 / ISO/IEC 14492 section 7.4.7.2 defines this bit as `TPGRON`;
        /// section 6.3.2 describes how it affects row decoding.
        const TPGRON = 1 << 1;
    }
}

/// Decoded JBIG2 generic refinement-region segment.
///
/// T.88 / ISO/IEC 14492 section 7.4.7 defines a generic refinement region as
/// a decoded bitmap plus the common region segment information from section
/// 7.4.1, which controls page placement and composition.
pub(crate) struct DecodedGenericRefinementRegionSegment {
    /// Decoded refinement bitmap produced by the section 6.3.2 procedure.
    pub(crate) image: JBig2Image,
    /// Region placement and composition metadata from section 7.4.1.
    region: RegionInfo,
}

impl DecodedGenericRefinementRegionSegment {
    /// Compose the decoded refinement bitmap into `dst` at the region position.
    ///
    /// T.88 / ISO/IEC 14492 section 8.2 defines page image composition, while
    /// section 7.4.1 supplies the region offset and external combination
    /// operator used here.
    pub(crate) fn compose_clipped_to(&self, dst: &mut JBig2Image) {
        self.image.compose_clipped_to(
            dst,
            i32::from(self.region.x),
            i32::from(self.region.y),
            ComposeOp::from(self.region.flags),
        );
    }
}

/// Decode one JBIG2 generic refinement-region segment.
///
/// T.88 / ISO/IEC 14492 section 7.4.7 defines the segment syntax and reference
/// bitmap dependency; section 6.3.2 defines the arithmetic refinement bitmap
/// procedure.
pub(crate) fn decode_generic_refinement_region_segment(
    context: &mut SegmentDecodeContext<'_, '_, '_, '_, '_>,
    fallback_reference: Option<&JBig2Image>,
) -> Result<DecodedGenericRefinementRegionSegment, Jbig2Error> {
    let header = GenericRefinementRegionHeader::parse(context.stream())?;
    let reference = context.referred_image_or(fallback_reference)?;
    let body = context.remaining_body(GENERIC_REFINEMENT_BODY)?;
    let mut stream = pdf_utils::BitReader::new(body);
    let mut decoder = JBig2ArithDecoder::new(&mut stream);
    let image = GenericRefinementRegionDecode::new(
        header.region.width,
        header.region.height,
        header.template,
        header.tpgron,
        header.at,
        0,
        0,
    )
    .decode(reference, &mut decoder)?;

    Ok(DecodedGenericRefinementRegionSegment {
        image,
        region: header.region,
    })
}

/// Parsed header state for a generic refinement region.
///
/// T.88 / ISO/IEC 14492 section 7.4.7 combines the common region segment
/// information field from section 7.4.1, generic refinement flags from section
/// 7.4.7.2, and optional adaptive-template bytes.
#[derive(Debug, Clone, Copy)]
struct GenericRefinementRegionHeader {
    /// Region dimensions, placement, and composition flags from section 7.4.1.
    region: RegionInfo,
    /// Generic refinement template selected by `GRTEMPLATE` in section 7.4.7.2.
    template: RefinementTemplate,
    /// Generic refinement line typical prediction flag from section 7.4.7.2.
    tpgron: bool,
    /// Optional adaptive-template coordinates from the section 7.4.7 segment data.
    at: RefinementAdaptiveTemplate,
}

impl GenericRefinementRegionHeader {
    /// Parse the generic refinement region header fields.
    ///
    /// T.88 / ISO/IEC 14492 section 7.4.7 defines the field order: region
    /// segment information, refinement flags, and optional adaptive-template
    /// bytes when template 0 is selected.
    fn parse(stream: &mut pdf_utils::BitReader<'_>) -> Result<Self, Jbig2Error> {
        let region = RegionInfo::parse(stream)?;
        let flags = GenericRefinementRegionFlagBits::from_bits_retain(stream.try_read_u8::<u8>()?);
        let template = RefinementTemplate::from_flag(
            flags.contains(GenericRefinementRegionFlagBits::TEMPLATE),
        );
        let at = RefinementAdaptiveTemplate::parse(stream, template)?;

        Ok(Self {
            region,
            template,
            tpgron: flags.contains(GenericRefinementRegionFlagBits::TPGRON),
            at,
        })
    }
}

/// Arithmetic generic-refinement bitmap decode parameters.
#[derive(Debug, Clone, Copy)]
pub(crate) struct GenericRefinementRegionDecode {
    width: u16,
    height: u16,
    template: RefinementTemplate,
    tpgron: bool,
    at: RefinementAdaptiveTemplate,
    reference_dx: i32,
    reference_dy: i32,
}

impl GenericRefinementRegionDecode {
    /// Create generic-refinement bitmap decode parameters.
    pub(crate) fn new(
        width: u16,
        height: u16,
        template: RefinementTemplate,
        tpgron: bool,
        at: RefinementAdaptiveTemplate,
        reference_dx: i32,
        reference_dy: i32,
    ) -> Self {
        Self {
            width,
            height,
            template,
            tpgron,
            at,
            reference_dx,
            reference_dy,
        }
    }

    /// Decode the refinement bitmap using a referenced bitmap.
    ///
    /// T.88 / ISO/IEC 14492 section 6.3.2 defines generic refinement decoding
    /// as arithmetic decoding from context bits drawn from the current bitmap
    /// and a reference bitmap. When `TPGRON` is enabled, each row first
    /// decodes the line typical prediction flag, then uniform 3x3 reference
    /// neighborhoods can predict pixels without consuming arithmetic data.
    pub(crate) fn decode(
        self,
        reference: &JBig2Image,
        decoder: &mut JBig2ArithDecoder<'_, '_>,
    ) -> Result<JBig2Image, Jbig2Error> {
        if !JBig2Image::is_valid_image_size(self.width, self.height) {
            return JBig2Image::try_new(self.width, self.height, None);
        }
        decoder.ensure_generic_region_contexts()?;
        let mut image = JBig2Image::try_new(self.width, self.height, None)?;
        let mut ltp = 0u8;
        for y in 0..self.height {
            if self.tpgron {
                ltp ^=
                    decoder.decode_prepared_generic_context(self.typical_prediction_context())?;
            }
            for x in 0..self.width {
                let pixel = if ltp != 0
                    && let Some(pixel) = self.implicit_refinement_pixel(reference, x, y)
                {
                    pixel
                } else {
                    let context = self.context_label(&image, reference, x, y);
                    decoder.decode_prepared_generic_context(context)?
                };
                image.set_pixel(x, y, pixel);
            }
        }
        Ok(image)
    }

    /// Return the arithmetic context used to decode `LTP` for this template.
    fn typical_prediction_context(self) -> usize {
        match self.template {
            RefinementTemplate::Template0 => 0x100,
            RefinementTemplate::Template1 => 0x40,
        }
    }

    /// Return an implicit refinement pixel when the aligned reference 3x3 area is uniform.
    fn implicit_refinement_pixel(self, reference: &JBig2Image, x: u16, y: u16) -> Option<u8> {
        let center = self.reference_pixel_at_offset(reference, x, y, 0, 0);
        for y_offset in -1..=1 {
            for x_offset in -1..=1 {
                if x_offset == 0 && y_offset == 0 {
                    continue;
                }
                if self.reference_pixel_at_offset(reference, x, y, x_offset, y_offset) != center {
                    return None;
                }
            }
        }
        Some(center)
    }

    /// Read one reference pixel after applying the refinement reference origin.
    fn reference_pixel_at_offset(
        self,
        reference: &JBig2Image,
        x: u16,
        y: u16,
        x_offset: i8,
        y_offset: i8,
    ) -> u8 {
        let Some((x, y)) = self.reference_coord(x, y, x_offset, y_offset) else {
            return 0;
        };
        reference.get_pixel(x, y)
    }

    /// Resolve an output coordinate to the corresponding reference coordinate.
    fn reference_coord(self, x: u16, y: u16, x_offset: i8, y_offset: i8) -> Option<(u16, u16)> {
        let x = i32::from(x)
            .checked_add(self.reference_dx)?
            .checked_add(i32::from(x_offset))
            .and_then(|value| u16::try_from(value).ok())?;
        let y = i32::from(y)
            .checked_add(self.reference_dy)?
            .checked_add(i32::from(y_offset))
            .and_then(|value| u16::try_from(value).ok())?;
        Some((x, y))
    }

    /// Build the arithmetic context label for one output pixel.
    ///
    /// T.88 / ISO/IEC 14492 section 6.3.2 defines the context label as the
    /// ordered current-region template bits followed by ordered reference-image
    /// template bits.
    fn context_label(self, image: &JBig2Image, reference: &JBig2Image, x: u16, y: u16) -> usize {
        let mut label = 0usize;
        for offset in self.template.coding_offsets(self.at) {
            label = (label << 1) | offset.pixel_from(image, x, y);
        }
        for offset in self.template.reference_offsets(self.at) {
            label = (label << 1)
                | offset.pixel_from_with_origin(
                    reference,
                    x,
                    y,
                    self.reference_dx,
                    self.reference_dy,
                );
        }
        label
    }
}

/// Generic refinement template selector.
///
/// T.88 / ISO/IEC 14492 section 7.4.7.2 selects between the two generic
/// refinement templates used by the section 6.3.2 arithmetic context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RefinementTemplate {
    /// Template 0, which can include adaptive-template pixels.
    ///
    /// T.88 / ISO/IEC 14492 section 6.3.2 defines this as the default generic
    /// refinement template, and section 7.4.7 includes its adaptive bytes.
    Template0,
    /// Template 1, the compact generic refinement template.
    ///
    /// T.88 / ISO/IEC 14492 section 6.3.2 defines this alternate template;
    /// section 7.4.7.2 selects it through `GRTEMPLATE`.
    Template1,
}

impl RefinementTemplate {
    /// Convert the `GRTEMPLATE` flag bit into a template variant.
    ///
    /// T.88 / ISO/IEC 14492 section 7.4.7.2 defines `GRTEMPLATE = 0` as
    /// template 0 and `GRTEMPLATE = 1` as template 1.
    pub(crate) fn from_flag(template_flag: bool) -> Self {
        if template_flag {
            Self::Template1
        } else {
            Self::Template0
        }
    }

    /// Return the current-region offsets for this template.
    ///
    /// T.88 / ISO/IEC 14492 section 6.3.2 orders the current-region template
    /// pixels before the reference-image pixels in the arithmetic context; for
    /// template 0 the adaptive coding pixel is appended.
    fn coding_offsets(
        self,
        at: RefinementAdaptiveTemplate,
    ) -> impl Iterator<Item = RefinementOffset> {
        let base: &'static [RefinementOffset] = match self {
            Self::Template0 => &TEMPLATE0_CODING,
            Self::Template1 => &TEMPLATE1_CODING,
        };
        base.iter().copied().chain(at.coding_offset(self))
    }

    /// Return the reference-image offsets for this template.
    ///
    /// T.88 / ISO/IEC 14492 section 6.3.2 appends the reference bitmap
    /// template pixels after the current-region pixels; for template 0 the
    /// adaptive reference pixel is appended.
    fn reference_offsets(
        self,
        at: RefinementAdaptiveTemplate,
    ) -> impl Iterator<Item = RefinementOffset> {
        let base: &'static [RefinementOffset] = match self {
            Self::Template0 => &TEMPLATE0_REFERENCE,
            Self::Template1 => &TEMPLATE1_REFERENCE,
        };
        base.iter().copied().chain(at.reference_offset(self))
    }
}

/// Optional adaptive-template pixels for refinement template 0.
///
/// T.88 / ISO/IEC 14492 section 7.4.7 carries adaptive-template coordinates
/// for generic refinement template 0; section 6.3.2 uses those coordinates as
/// additional arithmetic context pixels.
#[derive(Debug, Clone, Copy)]
pub(crate) struct RefinementAdaptiveTemplate {
    /// Optional current-region adaptive context pixel from section 7.4.7.
    coding: Option<RefinementOffset>,
    /// Optional reference-image adaptive context pixel from section 7.4.7.
    reference: Option<RefinementOffset>,
}

impl RefinementAdaptiveTemplate {
    /// Return the default adaptive template values used outside segment syntax.
    pub(crate) fn default_for(template: RefinementTemplate) -> Self {
        match template {
            RefinementTemplate::Template0 => Self {
                coding: Some(RefinementOffset { x: -1, y: -1 }),
                reference: Some(RefinementOffset { x: -1, y: -1 }),
            },
            RefinementTemplate::Template1 => Self {
                coding: None,
                reference: None,
            },
        }
    }

    /// Parse template-dependent adaptive-template coordinates.
    ///
    /// T.88 / ISO/IEC 14492 section 7.4.7 includes adaptive-template bytes for
    /// template 0 only; template 1 has no adaptive bytes.
    pub(crate) fn parse(
        stream: &mut pdf_utils::BitReader<'_>,
        template: RefinementTemplate,
    ) -> Result<Self, Jbig2Error> {
        if template == RefinementTemplate::Template1 {
            return Ok(Self {
                coding: None,
                reference: None,
            });
        }

        Ok(Self {
            coding: Some(RefinementOffset::parse(stream)?),
            reference: Some(RefinementOffset::parse(stream)?),
        })
    }

    /// Return the adaptive current-region offset for the selected template.
    ///
    /// T.88 / ISO/IEC 14492 section 6.3.2 uses this additional current-region
    /// context pixel only when template 0 is selected.
    fn coding_offset(self, template: RefinementTemplate) -> Option<RefinementOffset> {
        if template == RefinementTemplate::Template0 {
            self.coding
        } else {
            None
        }
    }

    /// Return the adaptive reference-image offset for the selected template.
    ///
    /// T.88 / ISO/IEC 14492 section 6.3.2 uses this additional reference
    /// context pixel only when template 0 is selected.
    fn reference_offset(self, template: RefinementTemplate) -> Option<RefinementOffset> {
        if template == RefinementTemplate::Template0 {
            self.reference
        } else {
            None
        }
    }
}

/// Signed pixel offset used by generic refinement templates.
///
/// T.88 / ISO/IEC 14492 section 6.3.2 defines template coordinates relative to
/// the current output pixel; section 7.4.7 stores adaptive-template
/// coordinates as signed byte pairs.
#[derive(Debug, Clone, Copy)]
struct RefinementOffset {
    /// Horizontal template offset from the current pixel, per section 6.3.2.
    x: i8,
    /// Vertical template offset from the current pixel, per section 6.3.2.
    y: i8,
}

impl RefinementOffset {
    /// Parse one signed adaptive-template coordinate pair.
    ///
    /// T.88 / ISO/IEC 14492 section 7.4.7 stores generic refinement adaptive
    /// template coordinates as signed byte `x, y` pairs.
    fn parse(stream: &mut pdf_utils::BitReader<'_>) -> Result<Self, Jbig2Error> {
        Ok(Self {
            x: stream.try_read_i8()?,
            y: stream.try_read_i8()?,
        })
    }

    /// Read the template pixel selected by this offset.
    ///
    /// T.88 / ISO/IEC 14492 section 6.3.2 treats pixels outside the current or
    /// reference bitmap bounds as zero while building the arithmetic context.
    fn pixel_from(self, image: &JBig2Image, x: u16, y: u16) -> usize {
        usize::from(image.pixel_at_offset(x, y, self.x, self.y))
    }

    /// Read the template pixel after applying a signed reference-image origin.
    fn pixel_from_with_origin(
        self,
        image: &JBig2Image,
        x: u16,
        y: u16,
        origin_x: i32,
        origin_y: i32,
    ) -> usize {
        let Some(x) = i32::from(x)
            .checked_add(origin_x)
            .and_then(|value| value.checked_add(i32::from(self.x)))
            .and_then(|value| u16::try_from(value).ok())
        else {
            return 0;
        };
        let Some(y) = i32::from(y)
            .checked_add(origin_y)
            .and_then(|value| value.checked_add(i32::from(self.y)))
            .and_then(|value| u16::try_from(value).ok())
        else {
            return 0;
        };
        usize::from(image.get_pixel(x, y))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        GenericRefinementRegionDecode, GenericRefinementRegionHeader, RefinementAdaptiveTemplate,
        RefinementTemplate,
    };
    use crate::image::JBig2Image;
    use pdf_utils::BitReader;

    fn header_bytes(flags: u8) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&1u32.to_be_bytes());
        data.extend_from_slice(&1u32.to_be_bytes());
        data.extend_from_slice(&0u32.to_be_bytes());
        data.extend_from_slice(&0u32.to_be_bytes());
        data.push(0);
        data.push(flags);
        data
    }

    #[test]
    fn template0_header_consumes_adaptive_template_bytes() {
        let mut data = header_bytes(0);
        data.extend_from_slice(&[0x00, 0xfe, 0xff, 0xff]);
        let mut stream = BitReader::new(&data);

        let header = GenericRefinementRegionHeader::parse(&mut stream).expect("header");

        assert_eq!(header.template, RefinementTemplate::Template0);
        assert_eq!(stream.byte_pos(), 22);
    }

    #[test]
    fn template1_header_has_no_adaptive_template_bytes() {
        let data = header_bytes(1);
        let mut stream = BitReader::new(&data);

        let header = GenericRefinementRegionHeader::parse(&mut stream).expect("header");

        assert_eq!(header.template, RefinementTemplate::Template1);
        assert_eq!(stream.byte_pos(), 18);
    }

    #[test]
    fn tpgron_no_longer_reports_unsupported() -> Result<(), crate::error::Jbig2Error> {
        let mut data = header_bytes(0b10);
        data.extend_from_slice(&[0, 0, 0, 0]);
        let mut stream = BitReader::new(&data);
        let header = GenericRefinementRegionHeader::parse(&mut stream)?;
        let mut body = BitReader::new(&[0xff; 16]);
        let mut decoder = crate::arith_decoder::JBig2ArithDecoder::new(&mut body);

        GenericRefinementRegionDecode::new(
            header.region.width,
            header.region.height,
            header.template,
            header.tpgron,
            header.at,
            0,
            0,
        )
        .decode(&JBig2Image::new(1, 1), &mut decoder)?;

        Ok(())
    }

    #[test]
    fn implicit_prediction_returns_uniform_reference_center_pixel() {
        let params = GenericRefinementRegionDecode::new(
            1,
            1,
            RefinementTemplate::Template1,
            true,
            RefinementAdaptiveTemplate::default_for(RefinementTemplate::Template1),
            1,
            1,
        );
        let mut reference = JBig2Image::new(3, 3);
        for y in 0..3 {
            for x in 0..3 {
                reference.set_pixel(x, y, 1);
            }
        }

        assert_eq!(params.implicit_refinement_pixel(&reference, 0, 0), Some(1));
    }

    #[test]
    fn implicit_prediction_returns_none_for_non_uniform_reference_neighborhood() {
        let params = GenericRefinementRegionDecode::new(
            1,
            1,
            RefinementTemplate::Template1,
            true,
            RefinementAdaptiveTemplate::default_for(RefinementTemplate::Template1),
            1,
            1,
        );
        let mut reference = JBig2Image::new(3, 3);
        for y in 0..3 {
            for x in 0..3 {
                reference.set_pixel(x, y, 1);
            }
        }
        reference.set_pixel(0, 0, 0);

        assert_eq!(params.implicit_refinement_pixel(&reference, 0, 0), None);
    }

    #[test]
    fn typical_prediction_context_depends_on_refinement_template() {
        let template0 = GenericRefinementRegionDecode::new(
            1,
            1,
            RefinementTemplate::Template0,
            true,
            RefinementAdaptiveTemplate::default_for(RefinementTemplate::Template0),
            0,
            0,
        );
        let template1 = GenericRefinementRegionDecode::new(
            1,
            1,
            RefinementTemplate::Template1,
            true,
            RefinementAdaptiveTemplate::default_for(RefinementTemplate::Template1),
            0,
            0,
        );

        assert_eq!(template0.typical_prediction_context(), 0x100);
        assert_eq!(template1.typical_prediction_context(), 0x40);
    }
}
