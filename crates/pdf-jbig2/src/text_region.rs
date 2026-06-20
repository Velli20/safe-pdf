//! JBIG2 text-region parsing and decoding.
//!
//! Text regions are defined by ITU-T T.88 | ISO/IEC 14492 section 7.4.3 and
//! decoded by the symbol-placement procedure in section 6.4. They are kept
//! separate from generic regions because they compose referenced symbol
//! dictionary bitmaps rather than directly decoding a generic bitmap.

mod arithmetic;
mod bitmap;
mod flags;
pub(crate) mod geometry;
mod huffman;
mod huffman_flags;
mod parser;
mod refinement;
pub(crate) mod state;
mod strip_decode_driver;

use crate::{
    decoded_region_segment::DecodedRegionSegment,
    error::Jbig2Error,
    segment_context::SegmentDecodeContext,
    text_region::{
        arithmetic::decode_arithmetic_text_region_segment, flags::TextRegionFlagBits,
        huffman::decode_huffman_text_region, parser::ParsedTextRegion,
    },
};

/// Decode one JBIG2 text-region segment.
///
/// ITU-T T.88 | ISO/IEC 14492 section 7.4.3.1 defines the text-region segment
/// header. The `SBHUFF` flag selects the Huffman procedure; otherwise the
/// arithmetic procedure is used.
pub(crate) fn decode_text_region_segment(
    context: &mut SegmentDecodeContext<'_, '_, '_, '_, '_>,
) -> Result<DecodedRegionSegment, Jbig2Error> {
    let parsed = ParsedTextRegion::parse(context)?;
    let region = parsed.region;
    let image = if parsed.flags.contains(TextRegionFlagBits::SBHUFF) {
        decode_huffman_text_region(context, parsed)?
    } else {
        decode_arithmetic_text_region_segment(context, parsed)?
    };
    Ok(DecodedRegionSegment { image, region })
}

#[cfg(test)]
mod tests {
    use crate::{
        decoded_region_segment::DecodedRegionSegment, image::JBig2Image, region_info::RegionInfo,
    };

    #[test]
    fn compose_clipped_to_places_text_region_bitmap_using_region_metadata() {
        let mut image = JBig2Image::new(1, 1);
        image.set_pixel(0, 0, 1);
        let decoded = DecodedRegionSegment {
            image,
            region: RegionInfo {
                width: 1,
                height: 1,
                x: 1,
                y: 1,
                flags: 2,
            },
        };
        let mut page = JBig2Image::new(3, 3);
        page.set_pixel(1, 1, 1);

        decoded.compose_clipped_to(&mut page);

        assert_eq!(page.get_pixel(1, 1), 0);
    }
}
