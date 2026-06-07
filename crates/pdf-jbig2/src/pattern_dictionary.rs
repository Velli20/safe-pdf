//! JBIG2 pattern dictionary segment decoding.
//!
//! ITU-T T.88 / ISO/IEC 14492 section 7.4.4 defines pattern dictionary
//! segments. They carry a horizontally concatenated collective bitmap that is
//! split into fixed-size pattern cells for later halftone-region composition.

mod adaptive_template;
mod collective_bitmap;
mod header;

use crate::{error::Jbig2Error, image::JBig2Image, segment_context::SegmentDecodeContext};

use self::{
    collective_bitmap::{decode_collective_bitmap, split_collective_bitmap},
    header::PatternDictionaryHeader,
};

/// Decoded JBIG2 pattern dictionary segment.
///
/// ITU-T T.88 / ISO/IEC 14492 section 7.4.4 defines pattern dictionaries as
/// the source of fixed-size pattern bitmaps referenced by halftone-region
/// segments in section 7.4.5.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct PatternDictionary {
    /// Width in pixels of every decoded pattern cell (`HDPW`).
    pub(crate) pattern_width: u8,
    /// Height in pixels of every decoded pattern cell (`HDPH`).
    pub(crate) pattern_height: u8,
    /// Decoded pattern cells ordered by their pattern dictionary index.
    pub(crate) patterns: Vec<JBig2Image>,
}

impl PatternDictionary {
    /// Decode one JBIG2 pattern dictionary segment from `context`.
    ///
    /// ITU-T T.88 / ISO/IEC 14492 section 7.4.4.1 defines the pattern
    /// dictionary header fields, and section 6.6 uses the resulting pattern
    /// images for halftone composition. This method parses the header, decodes
    /// the collective bitmap with the generic-region procedure, and splits the
    /// bitmap into individual fixed-size patterns.
    pub(crate) fn decode(
        context: &mut SegmentDecodeContext<'_, '_, '_, '_, '_>,
    ) -> Result<Self, Jbig2Error> {
        let header = PatternDictionaryHeader::parse(context.stream())?;
        let body = context.remaining_body("pattern dictionary data")?;
        let collective = decode_collective_bitmap(&header, body)?;
        let patterns = split_collective_bitmap(&collective, &header)?;

        Ok(Self {
            pattern_width: header.pattern_width(),
            pattern_height: header.pattern_height(),
            patterns,
        })
    }
}
