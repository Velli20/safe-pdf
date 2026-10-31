#![warn(missing_docs)]
//! Bounded decoding of JBIG2 (ITU-T T.88) segment streams.
//!
//! Borrowed parsing feeds owned dictionaries and region bitmaps, which are
//! composed into typestate-controlled pages. Standalone sequential/random-access
//! streams and PDF embedded streams share the same segment and codec boundaries.
//!
//! The PDF path decodes generic, refinement, halftone, and text regions,
//! pattern and symbol dictionaries in both entropy modes, retained symbol
//! dictionary contexts, unknown-length immediate generic regions, and striped
//! pages. Standalone containers and pages whose height is discovered from
//! their stripes report a typed unsupported-feature error.
//!
//! # Foreign and WebAssembly boundaries
//!
//! These are Rust APIs, not a stable C ABI. Adapters must own or pin input buffers
//! for the entire lifetime of borrowed views and copy or transfer owned bitmap
//! output explicitly. A JavaScript view must not survive relocation of WASM
//! linear memory.

/// Arithmetic entropy decoding and reusable probability contexts (T.88 Annex E).
pub mod arithmetic;
mod arithmetic_source;
/// Owned and borrowed bilevel pixels and composition contracts.
pub mod bitmap;
mod cursor;
/// Decoded dictionaries and typed segment-resource resolution.
pub mod dictionary;
/// Resource budgets, document orchestration, and page lifecycle states.
pub mod document;
/// Structured parsing, specification, resource, and I/O failures.
pub mod error;
/// Standard and custom Huffman tables and integer decoding (T.88 Annex B).
pub mod huffman;
mod huffman_source;
mod mmr;
/// Borrowed region descriptors and concrete region decoding stages.
pub mod region;
/// Zero-copy segment headers, references, and payloads (T.88 section 7).
pub mod segment;
/// Standalone and embedded stream framing (T.88 Annex D).
pub mod stream;
mod symbol_id_table;
mod symbols;
/// Owned adaptive templates for generic and refinement coding (T.88 section 6).
pub mod template;
mod text;
mod text_region;

pub use error::{Jbig2Error, Result};

/// Decode one PDF JBIG2 image to tightly packed, PDF-polarity bytes.
///
/// The dimensions come from the PDF image dictionary. Global segments, when
/// present, share the same cumulative decoding budget as the page stream.
pub fn decode_pdf(
    data: &[u8],
    width: u32,
    height: u32,
    globals: Option<&[u8]>,
    limits: document::DecodeLimits,
) -> Result<Vec<u8>> {
    use bitmap::DefaultCompositor;
    use document::{DecodeBudget, DocumentDecoder, Globals};

    if width == 0 || height == 0 {
        return Err(Jbig2Error::InvalidBitmap(
            "positive PDF dimensions required",
        ));
    }
    // The end of an embedded buffer completes a page that has no end-of-page
    // marker, but a buffer holding no segments at all encoded no image: that is
    // a truncated stream, not a blank page.
    if data.is_empty() {
        return Err(Jbig2Error::Truncated {
            location: error::SourceLocation {
                origin: error::InputOrigin::Embedded,
                byte_offset: 0,
                bit_offset: None,
            },
            needed: 1,
        });
    }
    let mut budget = DecodeBudget::new(limits);
    let global_resources = globals
        .map(|input| Globals::decode(input, &mut budget))
        .transpose()?;
    let mut decoder =
        DocumentDecoder::for_pdf(data, global_resources.as_ref(), width, height, &mut budget)?;
    let page = decoder
        .next_page(&mut DefaultCompositor, &mut budget)?
        .ok_or(Jbig2Error::InvalidPageSequence("PDF image has no page"))?;
    if decoder
        .next_page(&mut DefaultCompositor, &mut budget)?
        .is_some()
    {
        return Err(Jbig2Error::InvalidPageSequence(
            "PDF image has multiple pages",
        ));
    }
    page.into_bitmap().to_pdf_bytes(&mut budget)
}

#[cfg(test)]
mod tests {
    use super::{Result, decode_pdf};
    use crate::document::DecodeLimits;

    fn limits() -> DecodeLimits {
        DecodeLimits {
            input_bytes: 1000,
            segments: 100,
            references: 100,
            pixels: 100,
            allocation_bytes: 100_000,
            symbols: 100,
            work: 1000,
        }
    }

    #[test]
    fn pdf_entrypoint_decodes_headerless_one_pixel_page() -> Result<()> {
        let payload = [
            0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 3, 0xff, 0xfd, 0xff, 2, 0xfe,
            0xfe, 0xfe, 0,
        ];
        let mut data = vec![0, 0, 0, 1, 38, 0, 1, 0, 0, 0, 27];
        data.extend_from_slice(&payload);
        assert_eq!(decode_pdf(&data, 1, 1, None, limits())?, [0xff]);
        assert!(decode_pdf(&data, 0, 1, None, limits()).is_err());
        Ok(())
    }

    #[test]
    fn pdf_entrypoint_decodes_mmr_black_pixel() -> Result<()> {
        let payload = [
            0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0x26, 0xa8,
        ];
        let mut data = vec![0, 0, 0, 1, 38, 0, 1, 0, 0, 0, 20];
        data.extend_from_slice(&payload);
        assert_eq!(decode_pdf(&data, 1, 1, None, limits())?, [0x7f]);
        Ok(())
    }

    #[test]
    fn pdf_entrypoint_composes_single_global_halftone_pattern() -> Result<()> {
        let globals = [
            0, 0, 0, 1, 16, 0, 0, 0, 0, 0, 9, 1, 1, 1, 0, 0, 0, 0, 0x26, 0xa8,
        ];
        let payload = [
            0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0,
        ];
        let mut data = vec![0, 0, 0, 2, 22, 0x20, 1, 1, 0, 0, 0, 38];
        data.extend_from_slice(&payload);
        assert_eq!(decode_pdf(&data, 1, 1, Some(&globals), limits())?, [0x7f]);
        Ok(())
    }
}
