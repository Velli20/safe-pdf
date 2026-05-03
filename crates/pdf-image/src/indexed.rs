//! Utilities for unpacking packed image samples and expanding indexed palettes.

use crate::PdfImageError;

/// Number of color components in RGB color space.
const RGB_COMPONENTS: usize = 3;
/// Bit mask for extracting the low nibble from a byte.
const MASK_4BIT: u8 = 0x0F;
/// Bit mask for extracting two bits from a byte.
const MASK_2BIT: u8 = 0x03;
/// Bit mask for extracting one bit from a byte.
const MASK_1BIT: u8 = 0x01;

/// Creates the indexed-image error for unsupported bit depths.
fn unsupported_indexed_bits(bits_per_component: usize) -> PdfImageError {
    PdfImageError::UnsupportedIndexedBits { bits_per_component }
}

/// Creates the generic image error for unsupported bit depths.
fn unsupported_image_bits(bits_per_component: usize) -> PdfImageError {
    PdfImageError::UnsupportedImageBitsPerComponent { bits_per_component }
}

/// Reads one packed sample from a bit stream and advances the current bit position.
fn read_packed_sample(
    data: &[u8],
    bits_per_component: usize,
    bit_pos: &mut usize,
    unsupported_bits: fn(usize) -> PdfImageError,
) -> Result<u8, PdfImageError> {
    let byte_index = *bit_pos / 8;
    let bit_offset = *bit_pos % 8;
    let byte = *data
        .get(byte_index)
        .ok_or_else(|| PdfImageError::TruncatedImageData {
            expected_bytes: byte_index.saturating_add(1),
            actual_bytes: data.len(),
        })?;

    let value = match bits_per_component {
        8 => u32::from(byte),
        4 => u32::from((byte >> (4usize.saturating_sub(bit_offset))) & MASK_4BIT),
        2 => u32::from((byte >> (6usize.saturating_sub(bit_offset))) & MASK_2BIT),
        1 => u32::from((byte >> (7usize.saturating_sub(bit_offset))) & MASK_1BIT),
        _ => return Err(unsupported_bits(bits_per_component)),
    };

    *bit_pos = bit_pos.saturating_add(bits_per_component);
    u8::try_from(value).map_err(|_| {
        PdfImageError::InvalidImageData("packed sample value cannot fit in a byte".to_string())
    })
}

/// Unpacks image samples from a packed PDF row layout into one byte per sample.
fn unpack_samples(
    data: &[u8],
    width: usize,
    height: usize,
    bits_per_component: usize,
    samples_per_pixel: usize,
    unsupported_bits: fn(usize) -> PdfImageError,
) -> Result<Vec<u8>, PdfImageError> {
    let samples_per_row = width.saturating_mul(samples_per_pixel);
    let bits_per_row = samples_per_row.saturating_mul(bits_per_component);
    let bytes_per_row = bits_per_row.saturating_add(7) / 8;
    let mut out = Vec::with_capacity(
        width
            .saturating_mul(height)
            .saturating_mul(samples_per_pixel),
    );

    for row in 0..height {
        let mut bit_pos = row.saturating_mul(bytes_per_row).saturating_mul(8);
        for _ in 0..samples_per_row {
            out.push(read_packed_sample(
                data,
                bits_per_component,
                &mut bit_pos,
                unsupported_bits,
            )?);
        }
    }

    Ok(out)
}

/// Unpacks generic image samples into one byte per component sample.
pub(crate) fn unpack_image_samples(
    data: &[u8],
    width: usize,
    height: usize,
    bits_per_component: usize,
    num_components: usize,
) -> Result<Vec<u8>, PdfImageError> {
    unpack_samples(
        data,
        width,
        height,
        bits_per_component,
        num_components,
        unsupported_image_bits,
    )
}

/// Unpacks indexed palette indices into one byte per pixel index.
fn unpack_indexed_samples(
    data: &[u8],
    width: usize,
    height: usize,
    bits_per_component: usize,
) -> Result<Vec<u8>, PdfImageError> {
    unpack_samples(
        data,
        width,
        height,
        bits_per_component,
        1,
        unsupported_indexed_bits,
    )
}

/// Expands indexed color values into palette component bytes.
pub(crate) fn expand_indexed_values_to_components(
    indexed_values: &[u8],
    lookup: &[u8],
    hival: u8,
    base_components: usize,
) -> Result<Vec<u8>, PdfImageError> {
    if base_components == 0 {
        return Err(PdfImageError::InvalidColorComponentCount);
    }

    let mut out = Vec::with_capacity(indexed_values.len().saturating_mul(base_components));
    for (pixel_idx, &index) in indexed_values.iter().enumerate() {
        let clamped_index = index.min(hival);
        let start = usize::from(clamped_index).saturating_mul(base_components);
        let end = start.saturating_add(base_components);
        let entry = lookup.get(start..end).ok_or_else(|| {
            PdfImageError::InvalidImageData(format!(
                "Palette index {} out of bounds at pixel {} (lookup table size: {})",
                clamped_index,
                pixel_idx,
                lookup.len()
            ))
        })?;
        out.extend_from_slice(entry);
    }

    Ok(out)
}

/// Expands indexed color image data to an arbitrary component count.
pub fn expand_indexed_to_components(
    indexed_data: &[u8],
    lookup: &[u8],
    width: usize,
    height: usize,
    bits_per_component: usize,
    hival: u8,
    base_components: usize,
) -> Result<Vec<u8>, PdfImageError> {
    let indices = unpack_indexed_samples(indexed_data, width, height, bits_per_component)?;
    expand_indexed_values_to_components(&indices, lookup, hival, base_components)
}

/// Expands indexed color image data to RGB format.
pub fn expand_indexed_to_rgb(
    indexed_data: &[u8],
    lookup: &[u8],
    width: usize,
    height: usize,
    bits_per_component: usize,
    hival: u8,
) -> Result<Vec<u8>, PdfImageError> {
    expand_indexed_to_components(
        indexed_data,
        lookup,
        width,
        height,
        bits_per_component,
        hival,
        RGB_COMPONENTS,
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn unpack_image_samples_1bit_width_multiple_of_8() {
        let data = [0b1011_0010u8];
        let out = unpack_image_samples(&data, 8, 1, 1, 1).unwrap();

        assert_eq!(out, [1, 0, 1, 1, 0, 0, 1, 0]);
    }

    #[test]
    fn unpack_image_samples_1bit_respects_row_padding() {
        let data = [0b1010_0000u8, 0b0110_0000u8];
        let out = unpack_image_samples(&data, 3, 2, 1, 1).unwrap();

        assert_eq!(out, [1, 0, 1, 0, 1, 1]);
    }

    #[test]
    fn unpack_image_samples_1bit_multi_component() {
        let data = [0b1101_1000u8];
        let out = unpack_image_samples(&data, 2, 1, 1, 3).unwrap();

        assert_eq!(out, [1, 1, 0, 1, 1, 0]);
    }

    #[test]
    fn unpack_image_samples_unsupported_bits_use_image_error() {
        let err = unpack_image_samples(&[0], 1, 1, 3, 1).expect_err("3-bpc direct samples fail");

        assert!(matches!(
            err,
            PdfImageError::UnsupportedImageBitsPerComponent {
                bits_per_component: 3
            }
        ));
    }

    #[test]
    fn test_expand_indexed_8bit() {
        let width = 2usize;
        let height = 2usize;
        let indexed_data = vec![0u8, 1u8, 2u8, 1u8];
        let lookup = vec![10u8, 11u8, 12u8, 20u8, 21u8, 22u8, 30u8, 31u8, 32u8];

        let out = expand_indexed_to_rgb(&indexed_data, &lookup, width, height, 8, 2).unwrap();
        let expected = vec![10, 11, 12, 20, 21, 22, 30, 31, 32, 20, 21, 22];
        assert_eq!(out, expected);
    }

    #[test]
    fn test_expand_indexed_4bit() {
        let width = 4usize;
        let height = 1usize;
        let indexed_data = vec![0x01u8, 0x23u8];
        let lookup = vec![
            1u8, 2u8, 3u8, 4u8, 5u8, 6u8, 7u8, 8u8, 9u8, 10u8, 11u8, 12u8,
        ];

        let out = expand_indexed_to_rgb(&indexed_data, &lookup, width, height, 4, 3).unwrap();
        let expected = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
        assert_eq!(out, expected);
    }

    #[test]
    fn test_expand_indexed_2bit() {
        let width = 4usize;
        let height = 1usize;
        let indexed_data = vec![0x1Bu8];
        let lookup = vec![
            1u8, 2u8, 3u8, 4u8, 5u8, 6u8, 7u8, 8u8, 9u8, 10u8, 11u8, 12u8,
        ];

        let out = expand_indexed_to_rgb(&indexed_data, &lookup, width, height, 2, 3).unwrap();
        let expected = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
        assert_eq!(out, expected);
    }

    #[test]
    fn test_expand_indexed_1bit() {
        let width = 8usize;
        let height = 1usize;
        let indexed_data = vec![0b1010_1100u8];
        let lookup = vec![10u8, 11u8, 12u8, 20u8, 21u8, 22u8];

        let out =
            expand_indexed_to_components(&indexed_data, &lookup, width, height, 1, 1, 3).unwrap();
        let expected = vec![
            20, 21, 22, 10, 11, 12, 20, 21, 22, 10, 11, 12, 20, 21, 22, 20, 21, 22, 10, 11, 12, 10,
            11, 12,
        ];
        assert_eq!(out, expected);
    }

    #[test]
    fn test_expand_indexed_1bit_multi_row_uses_row_padding() {
        let width = 3usize;
        let height = 2usize;
        let indexed_data = vec![0b1010_0000u8, 0b0110_0000u8];
        let lookup = vec![1u8, 2u8, 3u8, 4u8, 5u8, 6u8];

        let out =
            expand_indexed_to_components(&indexed_data, &lookup, width, height, 1, 1, 3).unwrap();
        let expected = vec![4, 5, 6, 1, 2, 3, 4, 5, 6, 1, 2, 3, 4, 5, 6, 4, 5, 6];
        assert_eq!(out, expected);
    }

    #[test]
    fn test_expand_indexed_hival_clamp() {
        let width = 1usize;
        let height = 1usize;
        let indexed_data = vec![2u8];
        let lookup = vec![100u8, 101u8, 102u8, 110u8, 111u8, 112u8];

        let out = expand_indexed_to_rgb(&indexed_data, &lookup, width, height, 8, 1).unwrap();
        let expected = vec![110u8, 111u8, 112u8];
        assert_eq!(out, expected);
    }

    #[test]
    fn test_expand_indexed_insufficient_data() {
        let width = 2usize;
        let height = 2usize;
        let indexed_data = vec![0u8, 1u8, 2u8];
        let lookup = vec![0u8, 0u8, 0u8, 1u8, 1u8, 1u8];

        let err = expand_indexed_to_rgb(&indexed_data, &lookup, width, height, 8, 1)
            .err()
            .unwrap();
        match err {
            PdfImageError::TruncatedImageData { .. } => {}
            _ => panic!("expected TruncatedImageData"),
        }
    }

    #[test]
    fn test_expand_indexed_lookup_oob() {
        let width = 1usize;
        let height = 1usize;
        let indexed_data = vec![2u8];
        let lookup = vec![0u8, 0u8, 0u8, 1u8, 1u8, 1u8];

        let err = expand_indexed_to_rgb(&indexed_data, &lookup, width, height, 8, 2)
            .err()
            .unwrap();
        match err {
            PdfImageError::InvalidImageData(_) => {}
            _ => panic!("expected InvalidImageData"),
        }
    }
}
