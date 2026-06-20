//! Shared decoded JBIG2 region-segment representation.
//!
//! Multiple region decoders produce the same logical payload: a decoded bitmap
//! together with the region placement and composition metadata used for page
//! composition.

use crate::{compose_op::ComposeOp, image::JBig2Image, region_info::RegionInfo};

/// Decoded JBIG2 region segment bitmap and placement metadata.
///
/// The decoded bitmap is composed onto the page using the region information
/// from section 7.4.1, which provides the placement coordinates and
/// composition operator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DecodedRegionSegment {
    /// Decoded bitmap produced by the region-specific decode procedure.
    pub(crate) image: JBig2Image,
    /// Region placement and page-composition metadata from section 7.4.1.
    pub(crate) region: RegionInfo,
}

impl DecodedRegionSegment {
    /// Compose the decoded bitmap into `dst` at the region position.
    ///
    /// Page image composition clips to the overlapping area and applies the
    /// region's composition operator.
    pub(crate) fn compose_clipped_to(&self, dst: &mut JBig2Image) {
        self.image.compose_clipped_to(
            dst,
            i32::from(self.region.x),
            i32::from(self.region.y),
            ComposeOp::from(self.region.flags),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::DecodedRegionSegment;
    use crate::{image::JBig2Image, region_info::RegionInfo};

    #[test]
    fn compose_clipped_to_uses_region_position_and_flags() {
        let image = JBig2Image::new(1, 1);
        let decoded = DecodedRegionSegment {
            image,
            region: RegionInfo {
                width: 1,
                height: 1,
                x: 1,
                y: 1,
                flags: 1,
            },
        };
        let mut dst = JBig2Image::new(3, 3);
        dst.set_pixel(1, 1, 1);

        decoded.compose_clipped_to(&mut dst);

        assert_eq!(dst.get_pixel(1, 1), 0);
        assert_eq!(dst.get_pixel(0, 0), 0);
    }
}
