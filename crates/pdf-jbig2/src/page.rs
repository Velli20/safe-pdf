//! JBIG2 page metadata.

use crate::error::Jbig2Error;
use bitflags::bitflags;
use pdf_utils::BitReader;

bitflags! {
    /// JBIG2 page information flags from spec section 7.4.8.5.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    struct PageInfoFlagBits: u8 {
        const DEFAULT_PIXEL_VALUE = 1 << 2;
    }
}

bitflags! {
    /// JBIG2 page striping information from spec section 7.4.8.6.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    struct PageStripingInfoBits: u16 {
        const MAX_STRIPE_SIZE_MASK = 0x7fff;
        const IS_STRIPED = 1 << 15;
    }
}

impl PageInfoFlagBits {
    fn default_pixel_value(self) -> bool {
        self.contains(Self::DEFAULT_PIXEL_VALUE)
    }
}

impl PageStripingInfoBits {
    fn is_striped(self) -> bool {
        self.contains(Self::IS_STRIPED)
    }

    fn max_stripe_size(self) -> usize {
        usize::from(self.bits() & Self::MAX_STRIPE_SIZE_MASK.bits())
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PageInfo {
    pub(crate) width: u16,
    pub(crate) height: u16,
    pub(crate) resolution_x: usize,
    pub(crate) resolution_y: usize,
    pub(crate) default_pixel_value: bool,
    pub(crate) is_striped: bool,
    pub(crate) max_stripe_size: usize,
}

impl TryFrom<&[u8]> for PageInfo {
    type Error = Jbig2Error;

    fn try_from(data: &[u8]) -> Result<Self, Self::Error> {
        let mut stream = BitReader::new(data);
        Self::parse(&mut stream)
    }
}

impl PageInfo {
    pub(crate) fn parse(stream: &mut BitReader<'_>) -> Result<Self, Jbig2Error> {
        let width = stream.try_read_u32_be::<u16>()?;
        let height = stream.try_read_u32_be::<u16>()?;
        let resolution_x = stream.try_read_u32_be::<usize>()?;
        let resolution_y = stream.try_read_u32_be::<usize>()?;
        let segment_flags = PageInfoFlagBits::from_bits_retain(stream.try_read_u8::<u8>()?);
        let striping_info =
            PageStripingInfoBits::from_bits_retain(stream.try_read_u16_be::<u16>()?);

        Ok(PageInfo {
            width,
            height,
            resolution_x,
            resolution_y,
            default_pixel_value: segment_flags.default_pixel_value(),
            is_striped: striping_info.is_striped(),
            max_stripe_size: striping_info.max_stripe_size(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{PageInfoFlagBits, PageStripingInfoBits};

    #[test]
    fn extracts_page_info_flags() {
        let flags = PageInfoFlagBits::from_bits_retain(1 << 2);
        assert!(flags.default_pixel_value());
    }

    #[test]
    fn extracts_page_striping_info() {
        let bits = PageStripingInfoBits::from_bits_retain((1 << 15) | 0x1234);
        assert!(bits.is_striped());
        assert_eq!(bits.max_stripe_size(), 0x1234);
    }
}
