//! JBIG2 text-region segment-header parser.

use crate::{
    error::Jbig2Error,
    region_info::RegionInfo,
    segment_context::SegmentDecodeContext,
    text_region::{flags::TextRegionFlagBits, huffman_flags::TextRegionHuffmanFlags},
};
use pdf_utils::BitReader;

const REFINEMENT_AT_BYTES: usize = 4;
const TEXT_REGION_BODY: &str = "text region body";

/// Parsed JBIG2 text-region segment header and body slice.
///
/// ITU-T T.88 | ISO/IEC 14492 section 7.4.3.1 defines this segment layout:
/// region information, text-region flags, optional Huffman flags, optional
/// refinement AT bytes, `SBNUMINSTANCES`, and the encoded segment body.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ParsedTextRegion<'a> {
    /// Region dimensions, origin, and page-composition flags from section 7.4.1.
    pub(crate) region: RegionInfo,
    /// Text-region flags from section 7.4.3.1.1.
    pub(crate) flags: TextRegionFlagBits,
    /// Optional Huffman table selectors from section 7.4.3.1.2.
    pub(crate) huffman_flags: Option<TextRegionHuffmanFlags>,
    /// `SBNUMINSTANCES`, the symbol-instance count used by section 6.4.5.
    pub(crate) symbol_instances: u32,
    /// Encoded text-region data after the segment header.
    pub(crate) body: &'a [u8],
}

impl<'a> TryFrom<&'a [u8]> for ParsedTextRegion<'a> {
    type Error = Jbig2Error;

    /// Parse a complete text-region segment payload from a byte slice.
    ///
    /// ITU-T T.88 | ISO/IEC 14492 section 7.4.3.1 defines the byte layout.
    fn try_from(data: &'a [u8]) -> Result<Self, Self::Error> {
        let mut stream = BitReader::new(data);
        Self::parse_from_reader(&mut stream, data.len())
    }
}

impl<'a> ParsedTextRegion<'a> {
    /// Parse a text-region segment header from the active segment context.
    ///
    /// ITU-T T.88 | ISO/IEC 14492 section 7.4.3.1 defines the syntax. The
    /// returned body slice is bounded by the segment end from the context.
    pub(crate) fn parse(
        context: &mut SegmentDecodeContext<'_, '_, 'a, '_, '_>,
    ) -> Result<Self, Jbig2Error> {
        let end_byte_pos = context.segment_end();
        Self::parse_from_reader(context.stream(), end_byte_pos)
    }

    /// Parse the text-region header fields from `stream`.
    ///
    /// ITU-T T.88 | ISO/IEC 14492 sections 7.4.3.1.1 and 7.4.3.1.2 define
    /// the flag fields. `end_byte_pos` bounds the body slice in the source.
    fn parse_from_reader(
        stream: &mut BitReader<'a>,
        end_byte_pos: usize,
    ) -> Result<Self, Jbig2Error> {
        let region = RegionInfo::parse(stream)?;
        let raw_flags = stream.try_read_u16_be::<u16>()?;
        let flags = TextRegionFlagBits::from_bits_retain(raw_flags);
        let huffman_flags = if flags.contains(TextRegionFlagBits::SBHUFF) {
            Some(TextRegionHuffmanFlags::try_from(
                stream.try_read_u16_be::<u16>()?,
            )?)
        } else {
            None
        };

        if flags.contains(TextRegionFlagBits::SBREFINE)
            && !flags.contains(TextRegionFlagBits::SBRTEMPLATE)
        {
            Self::skip_refinement_at_bytes(stream)?;
        }

        let symbol_instances = stream.try_read_u32_be::<u32>()?;
        let body = stream
            .remaining_from_byte_until(end_byte_pos)
            .ok_or(Jbig2Error::Truncated(TEXT_REGION_BODY))?;

        Ok(Self {
            region,
            flags,
            huffman_flags,
            symbol_instances,
            body,
        })
    }

    /// Validate the currently supported Huffman text-region subset.
    ///
    /// ITU-T T.88 | ISO/IEC 14492 section 7.4.3 permits arithmetic and
    /// refinement text regions, but this decoder currently supports only
    /// non-refinement Huffman text regions on the Huffman path.
    pub(crate) fn validate_supported_huffman_text_region(&self) -> Result<(), Jbig2Error> {
        if !self.flags.contains(TextRegionFlagBits::SBHUFF) {
            return Err(Jbig2Error::UnsupportedFeature("arithmetic text regions"));
        }
        if self.flags.contains(TextRegionFlagBits::SBREFINE) {
            return Err(Jbig2Error::UnsupportedFeature("refinement text regions"));
        }
        Ok(())
    }

    /// Skip refinement adaptive-template bytes when `SBREFINE = 1` and `SBRTEMPLATE = 0`.
    ///
    /// ITU-T T.88 | ISO/IEC 14492 section 7.4.3.1.1 stores four bytes of
    /// refinement AT coordinates for that flag combination.
    fn skip_refinement_at_bytes(stream: &mut BitReader<'a>) -> Result<(), Jbig2Error> {
        for _ in 0..REFINEMENT_AT_BYTES {
            let _ = stream.try_read_u8::<u8>()?;
        }
        Ok(())
    }
}
