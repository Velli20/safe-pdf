//! JBIG2 text-region bitmap initialization.

use crate::{
    error::Jbig2Error,
    image::JBig2Image,
    text_region::{flags::TextRegionFlagBits, parser::ParsedTextRegion},
};

/// Initialize the text-region bitmap to `SBDEFPIXEL`.
///
/// ITU-T T.88 | ISO/IEC 14492 section 6.4.5 step 1 initializes the region
/// bitmap before decoded symbol instances are composed into it.
pub(crate) fn initialized_region(parsed: &ParsedTextRegion<'_>) -> Result<JBig2Image, Jbig2Error> {
    JBig2Image::try_new(
        parsed.region.width,
        parsed.region.height,
        Some(parsed.flags.contains(TextRegionFlagBits::SBDEFPIXEL)),
    )
}
