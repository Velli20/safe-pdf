//! Utilities for expanding indexed image data.

use pdf_decode::{SampleLayout, decode_sample_bytes, expand_indexed_values};

use crate::PdfImageError;

/// Number of color components in RGB color space.
const RGB_COMPONENTS: usize = 3;

fn decode_indices(
    indexed_data: &[u8],
    width: usize,
    height: usize,
    bits_per_component: usize,
) -> Result<Vec<u8>, PdfImageError> {
    Ok(decode_sample_bytes(
        indexed_data,
        bits_per_component,
        SampleLayout::RowAligned {
            width,
            height,
            samples_per_pixel: 1,
        },
    )?)
}

/// Expands indexed color values into palette component bytes.
pub(crate) fn expand_indexed_values_to_components(
    indexed_values: &[u8],
    lookup: &[u8],
    hival: u8,
    base_components: usize,
) -> Result<Vec<u8>, PdfImageError> {
    Ok(expand_indexed_values(
        indexed_values,
        lookup,
        hival,
        base_components,
    )?)
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
    let indices = decode_indices(indexed_data, width, height, bits_per_component)?;
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
        let width = 4usize;
        let height = 2usize;
        let indexed_data = vec![0b1010_0000u8, 0b0110_0000u8];
        let lookup = vec![11u8, 22u8, 33u8, 44u8, 55u8, 66u8];

        let out =
            expand_indexed_to_components(&indexed_data, &lookup, width, height, 1, 1, 3).unwrap();

        assert_eq!(
            out,
            vec![
                44, 55, 66, 11, 22, 33, 44, 55, 66, 11, 22, 33, 11, 22, 33, 44, 55, 66, 44, 55, 66,
                11, 22, 33
            ]
        );
    }

    #[test]
    fn test_expand_indexed_hival_clamp() {
        let out = expand_indexed_to_rgb(&[2u8], &[10, 11, 12, 20, 21, 22], 1, 1, 8, 1).unwrap();

        assert_eq!(out, vec![20, 21, 22]);
    }

    #[test]
    fn test_expand_indexed_insufficient_data() {
        let err = expand_indexed_to_rgb(&[0u8, 1u8, 2u8], &[10, 11, 12], 4, 1, 8, 1)
            .expect_err("indexed sample buffer is truncated");

        assert!(matches!(
            err,
            PdfImageError::TruncatedImageData {
                expected_bytes: 4,
                actual_bytes: 3
            }
        ));
    }

    #[test]
    fn test_expand_indexed_lookup_oob() {
        let err = expand_indexed_to_rgb(&[2u8], &[10, 11, 12], 1, 1, 8, 2)
            .expect_err("palette lookup should fail");

        assert!(matches!(err, PdfImageError::InvalidImageData(_)));
    }
}
