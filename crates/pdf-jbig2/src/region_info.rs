use crate::error::Jbig2Error;
use pdf_utils::BitReader;

/// JBIG2 region segment information field from ISO/IEC 14492 section 7.4.1.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct RegionInfo {
    /// Region segment bitmap width from ISO/IEC 14492 section 7.4.1.1.
    ///
    /// The bitstream encodes this as a four-byte pixel width, narrowed to the
    /// decoder's supported `u16` image size.
    pub(crate) width: u16,
    /// Region segment bitmap height from ISO/IEC 14492 section 7.4.1.2.
    ///
    /// The bitstream encodes this as a four-byte pixel height, narrowed to the
    /// decoder's supported `u16` image size.
    pub(crate) height: u16,
    /// Horizontal offset from ISO/IEC 14492 section 7.4.1.3.
    ///
    /// The bitstream encodes this as a four-byte unsigned offset relative to
    /// the page bitmap, narrowed to the decoder's supported `u16` coordinate
    /// range.
    pub(crate) x: u16,
    /// Vertical offset from ISO/IEC 14492 section 7.4.1.4.
    ///
    /// The bitstream encodes this as a four-byte unsigned offset relative to
    /// the page bitmap, narrowed to the decoder's supported `u16` coordinate
    /// range.
    pub(crate) y: u16,
    /// Region segment information flags from ISO/IEC 14492 section 7.4.1.5.
    ///
    /// Bits 0-2 encode the external combination operator, while bits 3-7 are
    /// reserved.
    pub(crate) flags: u8,
}

impl TryFrom<&[u8]> for RegionInfo {
    type Error = Jbig2Error;

    fn try_from(data: &[u8]) -> Result<Self, Self::Error> {
        let mut stream = BitReader::new(data);
        Self::parse(&mut stream)
    }
}

impl RegionInfo {
    pub(crate) fn parse(stream: &mut BitReader<'_>) -> Result<Self, Jbig2Error> {
        let width = stream.try_read_u32_be::<u16>()?;
        let height = stream.try_read_u32_be::<u16>()?;
        let x = stream.try_read_u32_be::<u16>()?;
        let y = stream.try_read_u32_be::<u16>()?;
        let flags = stream.try_read_u8::<u8>()?;

        Ok(Self {
            width,
            height,
            x,
            y,
            flags,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::RegionInfo;
    use crate::error::Jbig2Error;

    #[test]
    fn parse_region_info_header_success() {
        let data: [u8; 17] = [
            0x00, 0x00, 0x01, 0x20, // width = 288
            0x00, 0x00, 0x00, 0x10, // height = 16
            0x00, 0x00, 0x00, 0x08, // x = 8
            0x00, 0x00, 0x00, 0x04, // y = 4
            0xab, // flags
        ];

        let region = RegionInfo::try_from(data.as_slice()).expect("parse");
        assert_eq!(region.width, 288);
        assert_eq!(region.height, 16);
        assert_eq!(region.x, 8);
        assert_eq!(region.y, 4);
        assert_eq!(region.flags, 0xab);
    }

    #[test]
    fn parse_region_info_truncated_input_returns_typed_error() {
        let err = RegionInfo::try_from([0x00, 0x00, 0x01].as_slice()).expect_err("error");
        assert_eq!(err, Jbig2Error::Truncated("byte-aligned read"));
    }

    #[test]
    fn parse_region_info_rejects_coordinate_overflow() {
        let data = [
            0x00, 0x00, 0x00, 0x01, // width
            0x00, 0x00, 0x00, 0x02, // height
            0x00, 0x01, 0x00, 0x00, // x = 65_536
            0x00, 0x00, 0x00, 0x00, // y = 0
            0x01, // flags
        ];

        let err = RegionInfo::try_from(data.as_slice()).expect_err("overflow");
        assert_eq!(err, Jbig2Error::Overflow("integer conversion overflow"));
    }

    #[test]
    fn parse_region_info_accepts_max_coordinates() {
        let data = [
            0x00, 0x00, 0x00, 0x01, // width
            0x00, 0x00, 0x00, 0x02, // height
            0x00, 0x00, 0xff, 0xff, // x = 65_535
            0x00, 0x00, 0xff, 0xff, // y = 65_535
            0x01, // flags
        ];

        let region = RegionInfo::try_from(data.as_slice()).expect("parse");
        assert_eq!(region.x, u16::MAX);
        assert_eq!(region.y, u16::MAX);
    }
}
