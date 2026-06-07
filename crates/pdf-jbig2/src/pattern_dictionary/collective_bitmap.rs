use crate::{
    error::Jbig2Error,
    generic_region::{GenericRegion, GenericRegionTemplate, decode_mmr_region},
    image::JBig2Image,
};

use super::{adaptive_template::pattern_dictionary_template, header::PatternDictionaryHeader};

const FIRST_PATTERN_Y: u16 = 0;

/// Dimensions of a JBIG2 pattern dictionary collective bitmap.
///
/// ITU-T T.88 / ISO/IEC 14492 section 7.4.4 stores pattern bitmaps as one
/// collective bitmap with `GRAYMAX + 1` cells placed horizontally, each with
/// width `HDPW` and height `HDPH`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CollectiveBitmapDimensions {
    width: u16,
    height: u16,
    pattern_count: u32,
}

impl CollectiveBitmapDimensions {
    /// Compute checked collective bitmap dimensions from a parsed header.
    ///
    /// Section 7.4.4 implies collective width
    /// `(GRAYMAX + 1) * HDPW` and collective height `HDPH`. The decoder stores
    /// internal bitmap dimensions as `u16`, so this method rejects arithmetic
    /// or conversion overflow before allocating an image.
    fn from_header(header: &PatternDictionaryHeader) -> Result<Self, Jbig2Error> {
        let pattern_count = header.pattern_count()?;
        let width = pattern_count
            .checked_mul(u32::from(header.pattern_width()))
            .ok_or(Jbig2Error::Overflow("image dimensions overflow"))?;
        let width = u16::try_from(width)
            .map_err(|_| Jbig2Error::Overflow("integer conversion overflow"))?;

        Ok(Self {
            width,
            height: u16::from(header.pattern_height()),
            pattern_count,
        })
    }

    /// Return the X coordinate of a pattern cell inside the collective bitmap.
    ///
    /// Pattern cells are ordered left-to-right by index in the section 7.4.4
    /// collective bitmap, so `x = index * HDPW`.
    fn pattern_x(self, index: u32, pattern_width: u8) -> Result<u16, Jbig2Error> {
        let x = index
            .checked_mul(u32::from(pattern_width))
            .ok_or(Jbig2Error::Overflow("image dimensions overflow"))?;
        u16::try_from(x).map_err(|_| Jbig2Error::Overflow("integer conversion overflow"))
    }
}

/// Decode the pattern dictionary collective bitmap.
///
/// ITU-T T.88 / ISO/IEC 14492 section 7.4.4.2 supplies the collective bitmap
/// data. When `HDMMR` is set it is decoded as MMR; otherwise the arithmetic
/// generic-region procedure from section 6.2.5.7 is used with the
/// pattern-dictionary adaptive template.
pub(crate) fn decode_collective_bitmap(
    header: &PatternDictionaryHeader,
    body: &[u8],
) -> Result<JBig2Image, Jbig2Error> {
    let dimensions = CollectiveBitmapDimensions::from_header(header)?;
    if header.mmr() {
        decode_mmr_region(dimensions.width, dimensions.height, body)
    } else {
        decode_arithmetic_collective_bitmap(header, dimensions, body)
    }
}

/// Decode an arithmetic-coded pattern dictionary collective bitmap.
///
/// Section 7.4.4 reuses generic-region arithmetic decoding with `TPGDON` false
/// and the pattern-dictionary-specific adaptive-template coordinates.
fn decode_arithmetic_collective_bitmap(
    header: &PatternDictionaryHeader,
    dimensions: CollectiveBitmapDimensions,
    body: &[u8],
) -> Result<JBig2Image, Jbig2Error> {
    let template = GenericRegionTemplate::try_from(header.template())?;
    let gbat = pattern_dictionary_template(header.pattern_width(), template)?;
    GenericRegion::new_arithmetic(dimensions.width, dimensions.height, template, false, gbat)?
        .decode_arithmetic(body)
}

/// Split a collective bitmap into individual fixed-size pattern images.
///
/// ITU-T T.88 / ISO/IEC 14492 section 7.4.4 defines the decoded pattern
/// dictionary image as horizontally concatenated cells. This helper extracts
/// one subimage per index from zero through `GRAYMAX`.
pub(crate) fn split_collective_bitmap(
    collective: &JBig2Image,
    header: &PatternDictionaryHeader,
) -> Result<Vec<JBig2Image>, Jbig2Error> {
    let dimensions = CollectiveBitmapDimensions::from_header(header)?;
    let mut patterns = Vec::new();
    let pattern_count = usize::try_from(dimensions.pattern_count)
        .map_err(|_| Jbig2Error::Overflow("integer conversion overflow"))?;
    patterns
        .try_reserve_exact(pattern_count)
        .map_err(|_| Jbig2Error::Allocation("pattern images"))?;

    for index in 0..dimensions.pattern_count {
        let x = dimensions.pattern_x(index, header.pattern_width())?;
        patterns.push(collective.try_sub_image(
            x,
            FIRST_PATTERN_Y,
            u16::from(header.pattern_width()),
            dimensions.height,
        )?);
    }

    Ok(patterns)
}

#[cfg(test)]
mod tests {
    use super::{CollectiveBitmapDimensions, split_collective_bitmap};
    use crate::{
        error::Jbig2Error, image::JBig2Image, pattern_dictionary::header::PatternDictionaryHeader,
    };
    use pdf_utils::BitReader;

    fn header(data: [u8; 7]) -> PatternDictionaryHeader {
        let mut reader = BitReader::new(&data);
        PatternDictionaryHeader::parse(&mut reader).expect("header")
    }

    #[test]
    fn computes_collective_dimensions_from_pattern_count() {
        let header = header([0x00, 0x03, 0x02, 0x00, 0x00, 0x00, 0x05]);
        let dimensions = CollectiveBitmapDimensions::from_header(&header).expect("dimensions");

        assert_eq!(dimensions.width, 18);
        assert_eq!(dimensions.height, 2);
        assert_eq!(dimensions.pattern_count, 6);
    }

    #[test]
    fn rejects_collective_width_conversion_overflow() {
        let header = header([0x00, 0xff, 0x02, 0x00, 0x00, 0x01, 0x01]);

        assert_eq!(
            CollectiveBitmapDimensions::from_header(&header).expect_err("overflow"),
            Jbig2Error::Overflow("integer conversion overflow")
        );
    }

    #[test]
    fn splits_collective_bitmap_into_patterns() {
        let header = header([0x00, 0x02, 0x02, 0x00, 0x00, 0x00, 0x01]);
        let mut collective = JBig2Image::new(4, 2);
        collective.set_pixel(0, 0, 1);
        collective.set_pixel(1, 1, 1);
        collective.set_pixel(2, 0, 1);
        collective.set_pixel(3, 1, 1);

        let patterns = split_collective_bitmap(&collective, &header).expect("patterns");

        assert_eq!(patterns.len(), 2);
        assert_eq!(patterns.first().map(|image| image.get_pixel(0, 0)), Some(1));
        assert_eq!(patterns.first().map(|image| image.get_pixel(1, 1)), Some(1));
        assert_eq!(patterns.get(1).map(|image| image.get_pixel(0, 0)), Some(1));
        assert_eq!(patterns.get(1).map(|image| image.get_pixel(1, 1)), Some(1));
    }
}
