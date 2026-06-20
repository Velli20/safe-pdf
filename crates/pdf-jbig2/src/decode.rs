//! PDF `JBIG2Decode` public decode entrypoint.
//!
//! PDF image streams provide `Width`, `Height`, and optionally a
//! `JBIG2Globals` stream around the raw JBIG2 segment data. This module keeps
//! that PDF-facing orchestration separate from the T.88 / ISO/IEC 14492
//! segment walker in `stream`.

use crate::{error::Jbig2Error, image::JBig2Image, segment::ParsedSegment};

use super::stream::{decode_segments, decode_segments_with_prior};

const POSITIVE_PDF_IMAGE_DIMENSIONS: &str = "positive Width and Height";

/// Decoded output from one JBIG2 segment sequence.
///
/// T.88 / ISO/IEC 14492 section 7.2 defines a JBIG2 stream as an ordered
/// sequence of segments. The stream walker returns both the composed page image
/// from section 8.2 page composition and the retained decoded segments that
/// later page streams can reference through PDF `JBIG2Globals`.
#[derive(Debug)]
pub(crate) struct DecodedJbig2 {
    /// Page bitmap produced by JBIG2 page composition.
    pub(crate) page: JBig2Image,
    /// Decoded non-terminal segments retained for later segment references.
    #[allow(dead_code)]
    pub(crate) segments: Vec<ParsedSegment>,
}

/// Decode a PDF `JBIG2Decode` image stream to tightly packed PDF image bytes.
///
/// PDF supplies image `Width` and `Height` outside the JBIG2 segment stream and
/// can supply a separate `JBIG2Globals` stream whose decoded segments are prior
/// references for the page stream. T.88 / ISO/IEC 14492 section 7.2 defines the
/// segment stream model, and section 8.2 defines page composition before the
/// final PDF-facing bytes are emitted.
pub fn decode(
    data: &[u8],
    width: u16,
    height: u16,
    globals: Option<&[u8]>,
) -> Result<Vec<u8>, Jbig2Error> {
    Jbig2DecodeRequest::new(data, width, height, globals).decode_pdf_image()
}

/// PDF-facing JBIG2 decode request.
///
/// PDF `JBIG2Decode` wraps the JBIG2 page segment stream with external image
/// dimensions and optional global segments. This struct groups those inputs so
/// validation, global decoding, page decoding, and output conversion can be
/// tested independently of the public API shape.
struct Jbig2DecodeRequest<'data> {
    data: &'data [u8],
    width: u16,
    height: u16,
    globals: Option<&'data [u8]>,
}

impl<'data> Jbig2DecodeRequest<'data> {
    /// Build a request from the PDF image dictionary and stream bytes.
    ///
    /// PDF image dictionaries provide `Width` and `Height`; `globals` is the
    /// optional `JBIG2Globals` stream used as prior segment state by T.88
    /// section 7.2 segment references.
    fn new(data: &'data [u8], width: u16, height: u16, globals: Option<&'data [u8]>) -> Self {
        Self {
            data,
            width,
            height,
            globals,
        }
    }

    /// Decode the request into PDF image bytes.
    ///
    /// The JBIG2 page bitmap from T.88 section 8.2 is converted into tightly
    /// packed row bytes with PDF polarity after all global and page segments
    /// have been processed.
    fn decode_pdf_image(self) -> Result<Vec<u8>, Jbig2Error> {
        self.validate_dimensions()?;
        let globals = self.decode_globals()?;
        let decoded = self.decode_page(globals.as_deref().unwrap_or(&[]))?;
        Ok(Self::pdf_image_bytes(&decoded.page))
    }

    /// Validate the PDF-supplied page dimensions before allocating a page.
    ///
    /// PDF image `Width` and `Height` must describe a positive image. The
    /// decoder stores dimensions as `u16`, so only zero values need rejection at
    /// this boundary.
    fn validate_dimensions(&self) -> Result<(), Jbig2Error> {
        if self.width == 0 || self.height == 0 {
            return Err(Jbig2Error::InvalidState(POSITIVE_PDF_IMAGE_DIMENSIONS));
        }
        Ok(())
    }

    /// Decode optional PDF `JBIG2Globals` into prior JBIG2 segments.
    ///
    /// PDF global segments are a separate JBIG2 segment sequence. T.88 section
    /// 7.2.4 segment references can then resolve those decoded segments while
    /// the page stream is decoded.
    fn decode_globals(&self) -> Result<Option<Vec<ParsedSegment>>, Jbig2Error> {
        self.globals
            .map(|globals| decode_segments(globals, None).map(|decoded| decoded.segments))
            .transpose()
    }

    /// Decode the PDF page stream with already-decoded global segments.
    ///
    /// PDF provides page dimensions outside the JBIG2 stream, so they seed the
    /// page image before the T.88 section 7.2 segment walker processes `data`.
    fn decode_page(&self, prior_segments: &[ParsedSegment]) -> Result<DecodedJbig2, Jbig2Error> {
        decode_segments_with_prior(self.data, Some((self.width, self.height)), prior_segments)
    }

    /// Convert a composed JBIG2 page into PDF image bytes.
    ///
    /// T.88 section 8.2 composes into the decoder's internal 1-bit bitmap
    /// polarity. The PDF image pipeline consumes tightly packed rows with the
    /// opposite polarity, so the final conversion inverts while removing
    /// internal row alignment padding.
    fn pdf_image_bytes(page: &JBig2Image) -> Vec<u8> {
        page.inverted_tight_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::{POSITIVE_PDF_IMAGE_DIMENSIONS, decode};
    use crate::{error::Jbig2Error, segment::SegmentType};

    const PDF_EOF_SEGMENT_NUMBER: u32 = 1;
    const PDF_EOF_PAGE_ASSOCIATION: u8 = 1;
    const SEGMENT_TYPE_MASK: u8 = 0x3f;
    const NO_REFERRED_SEGMENTS: u8 = 0;
    const NO_SEGMENT_BODY_BYTES: u32 = 0;
    const ONE_BYTE_WIDE_IMAGE_WIDTH: u16 = 8;
    const SINGLE_ROW_IMAGE_HEIGHT: u16 = 1;
    const INVERTED_EMPTY_PAGE_BYTE: u8 = 0xff;

    fn minimal_end_of_file_stream() -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&PDF_EOF_SEGMENT_NUMBER.to_be_bytes());
        bytes.push(SegmentType::EndOfFile.code() & SEGMENT_TYPE_MASK);
        bytes.push(NO_REFERRED_SEGMENTS);
        bytes.push(PDF_EOF_PAGE_ASSOCIATION);
        bytes.extend_from_slice(&NO_SEGMENT_BODY_BYTES.to_be_bytes());
        bytes
    }

    #[test]
    fn zero_width_is_rejected_before_segment_decoding() {
        let err = decode(
            &[],
            0,
            SINGLE_ROW_IMAGE_HEIGHT,
            Some(&minimal_end_of_file_stream()),
        )
        .expect_err("zero width error");

        assert_eq!(err, Jbig2Error::InvalidState(POSITIVE_PDF_IMAGE_DIMENSIONS));
    }

    #[test]
    fn zero_height_is_rejected_before_segment_decoding() {
        let err = decode(
            &[],
            ONE_BYTE_WIDE_IMAGE_WIDTH,
            0,
            Some(&minimal_end_of_file_stream()),
        )
        .expect_err("zero height error");

        assert_eq!(err, Jbig2Error::InvalidState(POSITIVE_PDF_IMAGE_DIMENSIONS));
    }

    #[test]
    fn missing_globals_decode_uses_empty_prior_segments() {
        let decoded = decode(
            &minimal_end_of_file_stream(),
            ONE_BYTE_WIDE_IMAGE_WIDTH,
            SINGLE_ROW_IMAGE_HEIGHT,
            None,
        )
        .expect("decode without globals");

        assert_eq!(decoded, vec![INVERTED_EMPTY_PAGE_BYTE]);
    }

    #[test]
    fn empty_globals_decode_as_prior_segment_stream() {
        let globals = minimal_end_of_file_stream();
        let decoded = decode(
            &minimal_end_of_file_stream(),
            ONE_BYTE_WIDE_IMAGE_WIDTH,
            SINGLE_ROW_IMAGE_HEIGHT,
            Some(&globals),
        )
        .expect("decode with globals");

        assert_eq!(decoded, vec![INVERTED_EMPTY_PAGE_BYTE]);
    }
}
