//! Skip-bitmap policy for arithmetic generic-region decoding.
//!
//! ITU-T T.88 / ISO/IEC 14492 section 6.2.5.6 defines `SKIP` bitmap behavior:
//! skipped pixels are output as zero and are not arithmetically decoded.

use crate::image::JBig2Image;

/// Optional generic-region skip bitmap wrapper.
#[derive(Debug, Clone, Copy)]
pub(super) struct SkipBitmap<'a> {
    image: Option<&'a JBig2Image>,
}

impl<'a> SkipBitmap<'a> {
    /// Construct skip policy from an optional `SKIP` bitmap.
    pub(super) const fn new(image: Option<&'a JBig2Image>) -> Self {
        Self { image }
    }

    /// Return whether the pixel at `(col, row)` must be forced to zero.
    pub(super) fn is_skipped(self, col: u16, row: u16) -> bool {
        match self.image {
            Some(image) => image.get_pixel(col, row) != 0,
            None => false,
        }
    }
}
