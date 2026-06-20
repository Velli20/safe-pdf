mod header;

use self::header::HalftoneRegionHeader;
use crate::{
    arith_decoder::JBig2ArithDecoder,
    decoded_region_segment::DecodedRegionSegment,
    error::Jbig2Error,
    generic_region::{GenericRegion, GenericRegionAdaptiveTemplate, GenericRegionTemplate},
    image::JBig2Image,
    segment_context::SegmentDecodeContext,
};
use pdf_utils::BitReader;

const HALFTONE_REGION_BODY: &str = "halftone region body";
const MMR_HALFTONE_REGION: &str = "MMR halftone region";
const HALFTONE_GRAY_INDEX: &str = "halftone gray index";
const HALFTONE_PATTERN_INDEX: &str = "halftone pattern index";
const HALFTONE_BITS_PER_VALUE: &str = "halftone bits per value";
const HALFTONE_BITPLANE_WEIGHT: &str = "halftone bitplane weight";
const HALFTONE_GRAY_PLANES_ALLOCATION: &str = "halftone gray planes";

const HALFTONE_TEMPLATE01_ADAPTIVE_PIXELS: [i8; 8] = [3, -1, -3, -1, 2, -2, -2, -2];
const HALFTONE_TEMPLATE23_ADAPTIVE_PIXELS: [i8; 8] = [2, -1, -3, -1, 2, -2, -2, -2];

/// Decode one JBIG2 halftone-region segment from the current segment context.
///
/// ITU-T T.88 / ISO/IEC 14492 section 7.4.5 defines the segment header and
/// referred pattern dictionary requirements. Section 6.6.5 defines the
/// halftone-region decoding procedure used here for arithmetic-coded gray
/// images; MMR halftone gray images remain unsupported.
pub(crate) fn decode_halftone_region_segment(
    context: &mut SegmentDecodeContext<'_, '_, '_, '_, '_>,
) -> Result<DecodedRegionSegment, Jbig2Error> {
    let header = HalftoneRegionHeader::try_from(&mut *context.stream())?;
    if header.mmr {
        return Err(Jbig2Error::UnsupportedFeature(MMR_HALFTONE_REGION));
    }
    let patterns = context.referred_pattern_dictionary()?;

    let mut region_image = JBig2Image::try_new(
        header.region.width,
        header.region.height,
        Some(header.default_pixel),
    )?;
    let skip = if header.enable_skip {
        Some(header.compute_halftone_skip_map(
            u16::from(patterns.pattern_width),
            u16::from(patterns.pattern_height),
        )?)
    } else {
        None
    };
    let body = context.remaining_body(HALFTONE_REGION_BODY)?;
    let mut body_reader = BitReader::new(body);

    let gray_planes = HalftoneGrayPlanes::decode(
        &mut body_reader,
        header.grid_width,
        header.grid_height,
        header.template,
        skip.as_ref(),
        patterns.patterns.len(),
    )?;

    for cell in header.cells() {
        let placement = header.placement(cell)?;
        let gray_index = gray_planes.pattern_index(cell.ng, cell.mg)?;
        let pattern = selected_pattern(&patterns.patterns, gray_index)?;
        pattern.compose_clipped_to(
            &mut region_image,
            placement.x,
            placement.y,
            header.combination_operator,
        );
    }

    Ok(DecodedRegionSegment {
        image: region_image,
        region: header.region,
    })
}

/// Decode halftone gray values into a test-friendly row matrix.
///
/// ITU-T T.88 / ISO/IEC 14492 Annex C.5 defines the gray-scale image as
/// reconstructed bitplanes. Production composition computes indices directly
/// from [`HalftoneGrayPlanes`]; this helper keeps existing focused tests able
/// to assert complete gray-index images without duplicating bitplane logic.
#[cfg(test)]
fn decode_gray_indices<'stream, 'data>(
    stream: &'stream mut BitReader<'data>,
    grid_width: u16,
    grid_height: u16,
    template: u8,
    skip: Option<&JBig2Image>,
    pattern_count: usize,
) -> Result<Vec<Vec<usize>>, Jbig2Error> {
    HalftoneGrayPlanes::decode(
        stream,
        grid_width,
        grid_height,
        template,
        skip,
        pattern_count,
    )?
    .to_index_rows(grid_width, grid_height)
}

/// Decoded and reconstructed JBIG2 halftone gray-scale bitplanes.
///
/// ITU-T T.88 / ISO/IEC 14492 Annex C.5 defines the gray-scale image used by
/// halftone regions as `GSBPP` generic-region bitplanes. The encoded planes
/// are Gray-coded and are decoded from most-significant to least-significant
/// order; every lower plane is converted to its true binary value by XORing it
/// with the already-reconstructed next more-significant plane.
#[derive(Debug, Clone, PartialEq, Eq)]
struct HalftoneGrayPlanes {
    planes: Vec<JBig2Image>,
}

impl HalftoneGrayPlanes {
    /// Decode and reconstruct the halftone gray-scale bitplanes.
    ///
    /// ITU-T T.88 / ISO/IEC 14492 section 6.6.5 decodes the gray-scale image
    /// with the arithmetic generic-region procedure. Annex C.5 specifies that
    /// Gray-coded planes are decoded from most-significant to least-significant
    /// order and reconstructed by XORing each lower plane with the next higher
    /// reconstructed plane.
    fn decode<'stream, 'data>(
        stream: &'stream mut BitReader<'data>,
        grid_width: u16,
        grid_height: u16,
        template: u8,
        skip: Option<&JBig2Image>,
        pattern_count: usize,
    ) -> Result<Self, Jbig2Error> {
        let bits_per_value = usize::from(pattern_bits_per_value(pattern_count)?);
        let template = GenericRegionTemplate::try_from(template)?;
        let gbat = halftone_template(template);
        let region = GenericRegion::new_arithmetic(grid_width, grid_height, template, false, gbat)?;
        let mut decoder = JBig2ArithDecoder::new(stream);
        let mut planes = Vec::new();
        planes
            .try_reserve_exact(bits_per_value)
            .map_err(|_| Jbig2Error::Allocation(HALFTONE_GRAY_PLANES_ALLOCATION))?;

        for _ in (0..bits_per_value).rev() {
            let mut decoded = region.decode_arithmetic_with_decoder(&mut decoder, skip)?;
            apply_gray_code_plane_delta(&mut decoded, planes.last());
            planes.push(decoded);
        }
        planes.reverse();

        Ok(Self { planes })
    }

    /// Construct bitplanes that are already in least-significant-first order.
    ///
    /// ITU-T T.88 / ISO/IEC 14492 Annex C.5 names the reconstructed binary
    /// planes `GSPLANES[0] ... GSPLANES[GSBPP - 1]`; this constructor is used
    /// by unit tests for pattern-index assembly from those reconstructed
    /// planes.
    #[cfg(test)]
    fn from_reconstructed_planes(planes: Vec<JBig2Image>) -> Self {
        Self { planes }
    }

    /// Compute the selected pattern dictionary index for one gray-image pixel.
    ///
    /// ITU-T T.88 / ISO/IEC 14492 section 6.6 maps each halftone gray value to
    /// the pattern with the same dictionary index. Annex C.5 defines each set
    /// bit in `GSPLANES[plane]` as contributing the binary weight for `plane`.
    fn pattern_index(&self, x: u16, y: u16) -> Result<usize, Jbig2Error> {
        let mut value = 0usize;
        for (plane, image) in self.planes.iter().enumerate() {
            if image.get_pixel(x, y) == 0 {
                continue;
            }
            value = value
                .checked_add(bitplane_weight(plane)?)
                .ok_or(Jbig2Error::Overflow(HALFTONE_GRAY_INDEX))?;
        }
        Ok(value)
    }

    /// Materialize all pattern indices into row-major test data.
    ///
    /// ITU-T T.88 / ISO/IEC 14492 Annex C.5 represents the gray-scale image as
    /// bitplanes, but tests often need a direct row matrix for fixture
    /// assertions. This helper is test-only to keep production composition from
    /// allocating a duplicate gray-index image.
    #[cfg(test)]
    fn to_index_rows(
        &self,
        grid_width: u16,
        grid_height: u16,
    ) -> Result<Vec<Vec<usize>>, Jbig2Error> {
        let mut indices = Vec::new();
        indices
            .try_reserve_exact(usize::from(grid_height))
            .map_err(|_| Jbig2Error::Allocation(HALFTONE_GRAY_INDEX))?;

        for y in 0..grid_height {
            let mut row = Vec::new();
            row.try_reserve_exact(usize::from(grid_width))
                .map_err(|_| Jbig2Error::Allocation(HALFTONE_GRAY_INDEX))?;
            for x in 0..grid_width {
                row.push(self.pattern_index(x, y)?);
            }
            indices.push(row);
        }

        Ok(indices)
    }
}

/// Convert one decoded Gray-coded plane into its true binary plane value.
///
/// Annex C.5 states that a plane's true value is its coded value XORed with
/// the next more-significant bitplane. Because planes are processed from high
/// to low, that next plane has already been reconstructed when this helper is
/// called. The most-significant plane has no next plane and is left unchanged.
fn apply_gray_code_plane_delta(
    decoded: &mut JBig2Image,
    next_more_significant_plane: Option<&JBig2Image>,
) {
    if let Some(next_plane) = next_more_significant_plane {
        decoded.xor_from(next_plane);
    }
}

/// Return the halftone-specific adaptive template for gray-plane decoding.
///
/// ITU-T T.88 / ISO/IEC 14492 section 6.6.5 decodes the halftone gray-scale
/// image as arithmetic generic regions with adaptive-template positions fixed
/// by the selected `HTEMPLATE` value.
fn halftone_template(template: GenericRegionTemplate) -> GenericRegionAdaptiveTemplate {
    let normalized = match template {
        GenericRegionTemplate::Template0 | GenericRegionTemplate::Template1 => {
            HALFTONE_TEMPLATE01_ADAPTIVE_PIXELS
        }
        GenericRegionTemplate::Template2 | GenericRegionTemplate::Template3 => {
            HALFTONE_TEMPLATE23_ADAPTIVE_PIXELS
        }
    };
    GenericRegionAdaptiveTemplate::from_normalized(normalized)
}

/// Select the pattern dictionary entry for a decoded halftone gray value.
///
/// ITU-T T.88 / ISO/IEC 14492 section 6.6 maps gray values to pattern
/// dictionary entries. Values above the referred dictionary size are clamped to
/// the last available pattern, matching the decoder's existing tolerant
/// behavior for malformed streams.
fn selected_pattern(patterns: &[JBig2Image], gray_index: usize) -> Result<&JBig2Image, Jbig2Error> {
    patterns
        .get(gray_index.min(patterns.len().saturating_sub(1)))
        .ok_or(Jbig2Error::InvalidState(HALFTONE_PATTERN_INDEX))
}

/// Return `ceil(log2(pattern_count))`, the JBIG2 halftone bits per value.
///
/// T.88 section 6.6 sets `HBPP` from the number of patterns in the referred
/// pattern dictionary. A single-pattern dictionary needs no gray-image bits:
/// every decoded grid cell selects pattern index zero.
fn pattern_bits_per_value(pattern_count: usize) -> Result<u8, Jbig2Error> {
    let mut bits = 0u8;
    let mut representable_values = 1usize;
    while representable_values < pattern_count {
        bits = bits
            .checked_add(1)
            .ok_or(Jbig2Error::Overflow(HALFTONE_BITS_PER_VALUE))?;
        representable_values = representable_values
            .checked_mul(2)
            .ok_or(Jbig2Error::Overflow(HALFTONE_BITS_PER_VALUE))?;
    }
    Ok(bits)
}

/// Return the numeric contribution of a set bit in `GSPLANES[plane]`.
///
/// ITU-T T.88 / ISO/IEC 14492 Annex C.5 orders reconstructed planes from
/// least-significant to most-significant, so plane `n` contributes `2^n` to
/// the gray-scale value.
fn bitplane_weight(plane: usize) -> Result<usize, Jbig2Error> {
    let shift = u32::try_from(plane).map_err(|_| Jbig2Error::Overflow(HALFTONE_BITPLANE_WEIGHT))?;
    1usize
        .checked_shl(shift)
        .ok_or(Jbig2Error::Overflow(HALFTONE_BITPLANE_WEIGHT))
}

#[cfg(test)]
mod tests {
    use super::{
        HalftoneGrayPlanes, apply_gray_code_plane_delta, decode_gray_indices,
        pattern_bits_per_value, selected_pattern,
    };
    use crate::{
        decoded_region_segment::DecodedRegionSegment, error::Jbig2Error, image::JBig2Image,
        region_info::RegionInfo,
    };
    use pdf_utils::BitReader;

    fn decode_gray_indices_with_reader(
        mut reader: BitReader<'_>,
    ) -> Result<(Vec<Vec<usize>>, BitReader<'_>), Jbig2Error> {
        let indices = decode_gray_indices(&mut reader, 8, 4, 0, None, 2)?;
        Ok((indices, reader))
    }

    fn plane(width: u16, height: u16, pixels: &[(u16, u16)]) -> JBig2Image {
        let mut image = JBig2Image::new(width, height);
        for &(x, y) in pixels {
            image.set_pixel(x, y, 1);
        }
        image
    }

    #[test]
    fn pattern_bits_per_value_uses_specified_ceiling_log2() {
        // T.88 section 6.6 defines HBPP as ceil(log2(HNUMPATS)).
        assert_eq!(pattern_bits_per_value(1).expect("bits"), 0);
        assert_eq!(pattern_bits_per_value(2).expect("bits"), 1);
        assert_eq!(pattern_bits_per_value(3).expect("bits"), 2);
        assert_eq!(pattern_bits_per_value(4).expect("bits"), 2);
        assert_eq!(pattern_bits_per_value(5).expect("bits"), 3);
        assert_eq!(pattern_bits_per_value(8).expect("bits"), 3);
        assert_eq!(pattern_bits_per_value(9).expect("bits"), 4);
    }

    #[test]
    fn gray_code_plane_delta_xors_with_next_more_significant_plane() {
        // T.88 Annex C.5 reconstructs each Gray-coded plane by XORing it with
        // the next more-significant reconstructed plane.
        let next_plane = plane(4, 1, &[(0, 0), (3, 0)]);
        let mut decoded = plane(4, 1, &[(0, 0), (1, 0)]);

        apply_gray_code_plane_delta(&mut decoded, Some(&next_plane));

        assert_eq!(decoded.get_pixel(0, 0), 0);
        assert_eq!(decoded.get_pixel(1, 0), 1);
        assert_eq!(decoded.get_pixel(2, 0), 0);
        assert_eq!(decoded.get_pixel(3, 0), 1);
    }

    #[test]
    fn indices_from_bitplanes_assembles_weighted_gray_values() {
        let planes = vec![
            plane(3, 2, &[(0, 0), (1, 1)]),
            plane(3, 2, &[(1, 0), (1, 1)]),
            plane(3, 2, &[(2, 0), (1, 1)]),
        ];
        let gray_planes = HalftoneGrayPlanes::from_reconstructed_planes(planes);

        let indices = gray_planes.to_index_rows(3, 2).expect("indices");

        assert_eq!(indices, vec![vec![1, 2, 4], vec![0, 7, 0]]);
    }

    #[test]
    fn selected_pattern_clamps_out_of_range_gray_values_to_last_pattern() {
        let patterns = vec![
            plane(1, 1, &[]),
            plane(1, 1, &[(0, 0)]),
            plane(1, 1, &[(0, 0)]),
        ];

        let selected = selected_pattern(&patterns, 99).expect("pattern");

        assert_eq!(selected.get_pixel(0, 0), 1);
    }

    #[test]
    fn selected_pattern_rejects_empty_pattern_dictionary() {
        let patterns = Vec::new();

        let err = selected_pattern(&patterns, 0).expect_err("empty dictionary");

        assert_eq!(err, Jbig2Error::InvalidState("halftone pattern index"));
    }

    #[test]
    fn decode_gray_indices_advances_the_caller_reader() {
        let prefix = [0xaa, 0xbb, 0xcc];
        let body = [0x84, 0xc7, 0x73, 0xbf, 0xff, 0xac];
        let mut data = Vec::with_capacity(prefix.len() + body.len());
        data.extend_from_slice(&prefix);
        data.extend_from_slice(&body);

        let mut reader = BitReader::new(&data);
        reader.advance_bytes(prefix.len());

        let (indices, reader) = decode_gray_indices_with_reader(reader).expect("indices");

        assert_eq!(indices.len(), 4);
        assert!(indices.iter().all(|row| row.len() == 8));
        assert!(reader.byte_pos() > prefix.len());
    }

    #[test]
    fn compose_clipped_to_places_halftone_bitmap_using_region_metadata() {
        let mut image = JBig2Image::new(1, 1);
        image.set_pixel(0, 0, 1);
        let decoded = DecodedRegionSegment {
            image,
            region: RegionInfo {
                width: 1,
                height: 1,
                x: 1,
                y: 1,
                flags: 0,
            },
        };
        let mut dst = JBig2Image::new(3, 3);

        decoded.compose_clipped_to(&mut dst);

        assert_eq!(dst.get_pixel(1, 1), 1);
        assert_eq!(dst.get_pixel(0, 0), 0);
    }

    #[test]
    fn compose_clipped_to_uses_region_composition_operator() {
        let mut image = JBig2Image::new(1, 1);
        image.set_pixel(0, 0, 1);
        let decoded = DecodedRegionSegment {
            image,
            region: RegionInfo {
                width: 1,
                height: 1,
                x: 0,
                y: 0,
                flags: 1,
            },
        };
        let mut dst = JBig2Image::new(1, 1);
        dst.set_pixel(0, 0, 0);

        decoded.compose_clipped_to(&mut dst);

        assert_eq!(dst.get_pixel(0, 0), 0);
    }
}
