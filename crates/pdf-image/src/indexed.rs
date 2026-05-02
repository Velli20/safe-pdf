//! Utilities for expanding indexed (palette) image data into RGB bytes.

use crate::PdfImageError;

/// Number of color components in RGB color space.
const RGB_COMPONENTS: usize = 3;
/// Bit mask for extracting the low nibble (4 bits) from a byte.
const MASK_4BIT: u8 = 0x0F;
/// Bit mask for extracting 2 bits from a byte.
const MASK_2BIT: u8 = 0x03;
/// Bit mask for extracting 1 bit from a byte.
const MASK_1BIT: u8 = 0x01;

/// Extracts a color index value from packed bit data.
///
/// Indexed color images in PDF can use 1, 2, 4, or 8 bits per component.
/// This function extracts a single index value from the packed byte stream,
/// handling the bit-level addressing required for sub-byte bit depths.
///
/// # Parameters
///
/// - `data`: The raw packed image data bytes.
/// - `bits`: Bits per component (must be 1, 2, 4, or 8).
/// - `bit_pos`: Current bit position in the data stream; advanced by `bits` on success.
///
/// # Returns
///
/// The extracted index value in the range `[0, 2^bits - 1]`.
///
/// # Errors
///
/// - [`PdfCanvasError::InvalidImageData`] if `data` has insufficient bytes.
/// - [`PdfCanvasError::UnsupportedFeature`] if `bits` is not 1, 2, 4, or 8.
///
/// # Bit Packing Layout
///
/// Bits are packed from MSB to LSB within each byte:
///
/// | Depth | Layout per byte                         |
/// |-------|-----------------------------------------|
/// | 8-bit | `[b7..b0]` (entire byte)                |
/// | 4-bit | `[high_nibble \| low_nibble]`           |
/// | 2-bit | `[b7b6 \| b5b4 \| b3b2 \| b1b0]`        |
/// | 1-bit | `[b7 \| b6 \| b5 \| b4 \| b3 \| b2 \| b1 \| b0]` |
fn extract_index(data: &[u8], bits: usize, bit_pos: &mut usize) -> Result<u32, PdfImageError> {
    let byte_index = *bit_pos / 8;
    let bit_offset = *bit_pos % 8;

    let byte = *data.get(byte_index).ok_or_else(|| {
        // Report truncated data: expected at least `byte_index + 1` bytes
        PdfImageError::TruncatedImageData {
            expected_bytes: byte_index.saturating_add(1),
            actual_bytes: data.len(),
        }
    })?;

    let value = match bits {
        8 => u32::from(byte),
        4 => {
            // High nibble (bits 7–4) at offset 0, low nibble (bits 3–0) at offset 4
            let shift = 4_usize.saturating_sub(bit_offset);
            u32::from((byte >> shift) & MASK_4BIT)
        }
        2 => {
            // Extract 2-bit value; valid offsets are 0, 2, 4, 6
            let shift = 6_usize.saturating_sub(bit_offset);
            u32::from((byte >> shift) & MASK_2BIT)
        }
        1 => {
            // Extract single bit; valid offsets are 0–7
            let shift = 7_usize.saturating_sub(bit_offset);
            u32::from((byte >> shift) & MASK_1BIT)
        }
        _ => {
            return Err(PdfImageError::UnsupportedIndexedBits {
                bits_per_component: bits,
            });
        }
    };

    *bit_pos = bit_pos.saturating_add(bits);
    Ok(value)
}

/// Expands indexed color image data to an arbitrary component count.
///
/// Indexed (palette-based) images store each pixel as an index into a color
/// lookup table. This function decodes the packed index data and produces
/// a byte stream whose palette entries have `base_components` bytes each.
///
/// # Parameters
///
/// - `indexed_data`: Raw packed pixel indices.
/// - `lookup`: Color lookup table, `base_components` bytes per palette entry.
/// - `width`: Image width in pixels.
/// - `height`: Image height in pixels.
/// - `bits_per_component`: Bit depth of indices (1, 2, 4, or 8).
/// - `hival`: Maximum valid index value (indices are clamped to this).
/// - `base_components`: Number of bytes per palette entry.
///
/// # Returns
///
/// - `Ok(Vec<u8>)`: Expanded palette data (`width * height * base_components` bytes).
/// - `Err`: If data is malformed or insufficient.
///
/// # Example
///
/// For a 2x2 image with 4-bit indices and a 16-color palette:
/// ```text
/// Input:  [0x12, 0x34]  (indices: 1, 2, 3, 4)
/// Output: [C1..., C2..., C3..., C4...]
/// ```
pub fn expand_indexed_to_components(
    indexed_data: &[u8],
    lookup: &[u8],
    width: usize,
    height: usize,
    bits_per_component: usize,
    hival: u8,
    base_components: usize,
) -> Result<Vec<u8>, PdfImageError> {
    let num_pixels = width.saturating_mul(height);
    if base_components == 0 {
        return Err(PdfImageError::InvalidColorComponentCount);
    }

    let bits_per_row = width.saturating_mul(bits_per_component);
    let bytes_per_row = bits_per_row.saturating_add(7) / 8;
    let mut out = Vec::with_capacity(num_pixels.saturating_mul(base_components));

    for row in 0..height {
        let row_start_bit = row.saturating_mul(bytes_per_row).saturating_mul(8);
        let mut bit_pos = row_start_bit;
        for pixel_idx in 0..width {
            let index = extract_index(indexed_data, bits_per_component, &mut bit_pos)?;

            // Clamp index to valid palette range.
            let clamped_index = index.min(u32::from(hival));
            #[allow(clippy::as_conversions)]
            let clamped_index_usize = clamped_index as usize;
            let base = clamped_index_usize.saturating_mul(base_components);
            let end = base.saturating_add(base_components);

            let entry = lookup.get(base..end).ok_or_else(|| {
                PdfImageError::InvalidImageData(format!(
                    "Palette index {} out of bounds at pixel {} (lookup table size: {})",
                    clamped_index_usize,
                    row.saturating_mul(width).saturating_add(pixel_idx),
                    lookup.len()
                ))
            })?;

            out.extend_from_slice(entry);
        }
    }

    Ok(out)
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
mod tests {
    use super::*;

    #[test]
    fn test_expand_indexed_8bit() {
        // 2x2 image with indices [0,1,2,1]
        let width = 2usize;
        let height = 2usize;
        let indexed_data = vec![0u8, 1u8, 2u8, 1u8];
        // lookup: 3 palette entries
        let lookup = vec![
            10u8, 11u8, 12u8, // entry 0
            20u8, 21u8, 22u8, // entry 1
            30u8, 31u8, 32u8, // entry 2
        ];

        let out = expand_indexed_to_rgb(&indexed_data, &lookup, width, height, 8, 2).unwrap();
        let expected = vec![
            10, 11, 12, // 0
            20, 21, 22, // 1
            30, 31, 32, // 2
            20, 21, 22, // 1
        ];
        assert_eq!(out, expected);
    }

    #[test]
    fn test_expand_indexed_4bit() {
        let width = 4usize;
        let height = 1usize;
        let indexed_data = vec![0x01u8, 0x23u8];
        let lookup = vec![
            1u8, 2u8, 3u8, // 0
            4u8, 5u8, 6u8, // 1
            7u8, 8u8, 9u8, // 2
            10u8, 11u8, 12u8, // 3
        ];

        let out = expand_indexed_to_rgb(&indexed_data, &lookup, width, height, 4, 3).unwrap();
        let expected = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
        assert_eq!(out, expected);
    }

    #[test]
    fn test_expand_indexed_2bit() {
        // width=4,height=1 => 4 pixels packed into one byte: indices 0,1,2,3 -> 00 01 10 11 = 0x1B
        let width = 4usize;
        let height = 1usize;
        let indexed_data = vec![0x1Bu8];
        let lookup = vec![
            1u8, 2u8, 3u8, // 0
            4u8, 5u8, 6u8, // 1
            7u8, 8u8, 9u8, // 2
            10u8, 11u8, 12u8, // 3
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
        let lookup = vec![
            10u8, 11u8, 12u8, // 0
            20u8, 21u8, 22u8, // 1
        ];

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
        let lookup = vec![
            1u8, 2u8, 3u8, // 0
            4u8, 5u8, 6u8, // 1
        ];

        let out =
            expand_indexed_to_components(&indexed_data, &lookup, width, height, 1, 1, 3).unwrap();
        let expected = vec![
            4, 5, 6, 1, 2, 3, 4, 5, 6, // row 1: 1,0,1
            1, 2, 3, 4, 5, 6, 4, 5, 6, // row 2: 0,1,1
        ];
        assert_eq!(out, expected);
    }

    #[test]
    fn test_expand_indexed_hival_clamp() {
        // One pixel with index 2 but hival=1 -> should clamp to 1
        let width = 1usize;
        let height = 1usize;
        let indexed_data = vec![2u8];
        let lookup = vec![
            100u8, 101u8, 102u8, // 0
            110u8, 111u8, 112u8, // 1
        ];

        let out = expand_indexed_to_rgb(&indexed_data, &lookup, width, height, 8, 1).unwrap();
        let expected = vec![110u8, 111u8, 112u8];
        assert_eq!(out, expected);
    }

    #[test]
    fn test_expand_indexed_insufficient_data() {
        // 2x2 image but provide only 3 bytes for 8-bit data
        let width = 2usize;
        let height = 2usize;
        let indexed_data = vec![0u8, 1u8, 2u8]; // missing one byte
        let lookup = vec![0u8, 0u8, 0u8, 1u8, 1u8, 1u8];

        let err = expand_indexed_to_rgb(&indexed_data, &lookup, width, height, 8, 1)
            .err()
            .unwrap();
        match err {
            PdfImageError::TruncatedImageData {
                expected_bytes: _,
                actual_bytes: _,
            } => {}
            _ => panic!("expected TruncatedImageData"),
        }
    }

    #[test]
    fn test_expand_indexed_lookup_oob() {
        // One pixel index 2 but lookup only has two entries
        let width = 1usize;
        let height = 1usize;
        let indexed_data = vec![2u8];
        let lookup = vec![0u8, 0u8, 0u8, 1u8, 1u8, 1u8]; // entries 0 and 1 only

        let err = expand_indexed_to_rgb(&indexed_data, &lookup, width, height, 8, 2)
            .err()
            .unwrap();
        match err {
            PdfImageError::InvalidImageData(_) => {}
            _ => panic!("expected InvalidImageData"),
        }
    }
}
