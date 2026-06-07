//! Internal benchmark entrypoints for `pdf-jbig2`.

use crate::{
    arith_decoder::JBig2ArithDecoder,
    compose_op::ComposeOp,
    error::Jbig2Error,
    generic_region::{
        GenericRegion, GenericRegionAdaptiveTemplate, GenericRegionTemplate,
        tables::Template12Config,
    },
    huffman::{STANDARD_TABLE_B1, StandardHuffmanDecoder},
    image::JBig2Image,
};
use pdf_utils::BitReader;

const TEMPLATE0_DATA: &[u8] = &[0x84, 0xc7, 0x73, 0xbf, 0xff, 0xac];
const TEMPLATE2_DATA: &[u8] = &[0x9a, 0x33, 0x55, 0xe1, 0x0f, 0xff, 0xac];
const HUFFMAN_DATA: &[u8] = &[0; 256];

/// Benchmark the optimized template-0 generic-region path.
pub fn decode_optimized_template0() -> Result<usize, Jbig2Error> {
    let gbat = GenericRegionAdaptiveTemplate::from(&[], 0, true, GenericRegionTemplate::Template0)?;
    let region =
        GenericRegion::new_arithmetic(8, 4, GenericRegionTemplate::Template0, false, gbat)?;
    let image = region.decode_arithmetic(TEMPLATE0_DATA)?;
    Ok(checksum_bytes(image.data()))
}

/// Benchmark the optimized template-2 generic-region path.
pub fn decode_optimized_template2() -> Result<usize, Jbig2Error> {
    let gbat = GenericRegionAdaptiveTemplate::from(&[], 0, true, GenericRegionTemplate::Template2)?;
    let region =
        GenericRegion::new_arithmetic(8, 4, GenericRegionTemplate::Template2, false, gbat)?;
    let image = region.decode_arithmetic(TEMPLATE2_DATA)?;
    Ok(checksum_bytes(image.data()))
}

/// Benchmark the unoptimized generic-region path with a skip bitmap.
pub fn decode_unoptimized_with_skip() -> Result<usize, Jbig2Error> {
    let template =
        GenericRegionAdaptiveTemplate::from(&[], 0, true, GenericRegionTemplate::Template2)?;
    let mut skip = JBig2Image::try_new(8, 4, None)?;
    skip.set_pixel(1, 0, 1);
    skip.set_pixel(6, 2, 1);

    let mut stream = BitReader::new(TEMPLATE2_DATA);
    let mut decoder = JBig2ArithDecoder::new(&mut stream);
    let image = decoder.decode_arith_template12_unopt_skip(
        8,
        4,
        Template12Config::TEMPLATE2,
        false,
        &template,
        Some(&skip),
    )?;
    Ok(checksum_bytes(image.data()))
}

/// Benchmark the public arithmetic generic-region dispatch.
pub fn decode_generic_region_dispatch() -> Result<usize, Jbig2Error> {
    let gbat = GenericRegionAdaptiveTemplate::from(&[], 0, true, GenericRegionTemplate::Template0)?;
    let region =
        GenericRegion::new_arithmetic(8, 4, GenericRegionTemplate::Template0, false, gbat)?;
    let image = region.decode_arithmetic(TEMPLATE0_DATA)?;
    Ok(checksum_bytes(image.data()))
}

/// Benchmark aligned byte-span composition.
pub fn compose_aligned_bitmap() -> Result<usize, Jbig2Error> {
    let src = patterned_image(256, 128)?;
    let mut dst = patterned_image(256, 128)?;
    src.compose_clipped_to(&mut dst, 0, 0, ComposeOp::Xor);
    src.compose_clipped_to(&mut dst, 16, 16, ComposeOp::Or);
    src.compose_clipped_to(&mut dst, -8, 24, ComposeOp::Replace);
    Ok(checksum_bytes(dst.data()))
}

/// Benchmark byte-aligned collective-bitmap subimage extraction.
pub fn extract_aligned_subimages() -> Result<usize, Jbig2Error> {
    let collective = patterned_image(512, 64)?;
    let mut checksum = 0usize;
    for x in (0..512u16).step_by(16) {
        let image = collective.try_sub_image(x, 0, 16, 64)?;
        checksum = checksum.wrapping_add(checksum_bytes(image.data()));
    }
    Ok(checksum)
}

/// Benchmark tight output conversion with inversion.
pub fn invert_tight_output() -> Result<usize, Jbig2Error> {
    let image = patterned_image(399, 400)?;
    Ok(checksum_bytes(&image.inverted_tight_bytes()))
}

/// Benchmark standard Huffman decode loops.
pub fn decode_standard_huffman() -> Result<usize, Jbig2Error> {
    let decoder = StandardHuffmanDecoder::new(STANDARD_TABLE_B1)?;
    let mut reader = BitReader::new(HUFFMAN_DATA);
    let mut checksum = 0usize;
    while let Ok(value) = decoder.decode(&mut reader) {
        checksum = checksum.wrapping_add(match value {
            crate::huffman::HuffmanValue::Value(value) => {
                usize::try_from(value).unwrap_or_default()
            }
            crate::huffman::HuffmanValue::OutOfBand => 1,
        });
        if reader.exhausted() {
            break;
        }
    }
    Ok(checksum)
}

fn patterned_image(width: u16, height: u16) -> Result<JBig2Image, Jbig2Error> {
    let mut image = JBig2Image::try_new(width, height, None)?;
    for (index, byte) in image.data_mut().iter_mut().enumerate() {
        let low = u8::try_from(index & 0xff).unwrap_or_default();
        *byte = low.wrapping_mul(37).wrapping_add(0x5a);
    }
    Ok(image)
}

fn checksum_bytes(bytes: &[u8]) -> usize {
    bytes.iter().fold(0usize, |acc, byte| {
        acc.wrapping_mul(16_777_619)
            .wrapping_add(usize::from(*byte))
    })
}
