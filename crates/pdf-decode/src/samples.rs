//! Sample code unpacking and normalization helpers.

use num_traits::ToPrimitive;

use crate::{error::DecodeError, layout::SampleLayout};

/// Decodes packed sample codes into integer sample values.
pub fn decode_sample_codes(
    data: &[u8],
    bits_per_sample: usize,
    layout: SampleLayout,
) -> Result<Vec<u32>, DecodeError> {
    validate_bits_per_sample(bits_per_sample)?;

    match layout {
        SampleLayout::Contiguous { sample_count } => {
            let total_bits = sample_count.saturating_mul(bits_per_sample);
            let expected_bytes = total_bits.div_ceil(8);
            ensure_len(data, expected_bytes)?;

            let mut out = Vec::with_capacity(sample_count);
            let mut bit_offset = 0usize;
            for _ in 0..sample_count {
                out.push(read_bits(data, bit_offset, bits_per_sample)?);
                bit_offset = bit_offset.saturating_add(bits_per_sample);
            }
            Ok(out)
        }
        SampleLayout::RowAligned {
            width,
            height,
            samples_per_pixel,
        } => {
            let samples_per_row = width.saturating_mul(samples_per_pixel);
            let bits_per_row = samples_per_row.saturating_mul(bits_per_sample);
            let bytes_per_row = bits_per_row.div_ceil(8);
            let expected_bytes = height.saturating_mul(bytes_per_row);
            ensure_len(data, expected_bytes)?;

            let total_samples = width
                .saturating_mul(height)
                .saturating_mul(samples_per_pixel);
            let mut out = Vec::with_capacity(total_samples);
            for row in 0..height {
                let mut bit_offset = row.saturating_mul(bytes_per_row).saturating_mul(8);
                for _ in 0..samples_per_row {
                    out.push(read_bits(data, bit_offset, bits_per_sample)?);
                    bit_offset = bit_offset.saturating_add(bits_per_sample);
                }
            }
            Ok(out)
        }
    }
}

/// Decodes packed sample codes and normalizes them to the `0.0..=1.0` range.
pub fn decode_normalized_samples(
    data: &[u8],
    bits_per_sample: usize,
    count: usize,
) -> Result<Vec<f32>, DecodeError> {
    let samples = decode_sample_codes(
        data,
        bits_per_sample,
        SampleLayout::Contiguous {
            sample_count: count,
        },
    )?;
    let bits_u32 = u32::try_from(bits_per_sample)
        .map_err(|_| DecodeError::InvalidBitsPerSample { bits_per_sample })?;
    let max_value = 1u64
        .checked_shl(bits_u32)
        .ok_or(DecodeError::InvalidBitsPerSample { bits_per_sample })?
        .saturating_sub(1);
    let max_value_f32 = max_value.to_f32().ok_or(DecodeError::InvalidSampleData)?;

    let mut out = Vec::with_capacity(samples.len());
    for sample in samples {
        let sample_f32 = sample.to_f32().ok_or(DecodeError::InvalidSampleData)?;
        out.push(sample_f32 / max_value_f32);
    }

    Ok(out)
}

/// Validates that the requested bit width is one of the supported PDF sample sizes.
fn validate_bits_per_sample(bits_per_sample: usize) -> Result<(), DecodeError> {
    if matches!(bits_per_sample, 1 | 2 | 4 | 8 | 12 | 16 | 24 | 32) {
        return Ok(());
    }

    Err(DecodeError::InvalidBitsPerSample { bits_per_sample })
}

/// Ensures that the input contains at least the requested number of bytes.
fn ensure_len(data: &[u8], expected_bytes: usize) -> Result<(), DecodeError> {
    if data.len() < expected_bytes {
        return Err(DecodeError::InsufficientData {
            expected_bytes,
            actual_bytes: data.len(),
        });
    }

    Ok(())
}

/// Reads one packed sample value from the input bit stream.
fn read_bits(data: &[u8], bit_offset: usize, bits_per_sample: usize) -> Result<u32, DecodeError> {
    let mut value = 0u32;

    for bit_index in 0..bits_per_sample {
        let absolute_bit = bit_offset.saturating_add(bit_index);
        let byte_index = absolute_bit / 8;
        let bit_in_byte = absolute_bit % 8;
        let byte = *data
            .get(byte_index)
            .ok_or_else(|| DecodeError::InsufficientData {
                expected_bytes: byte_index.saturating_add(1),
                actual_bytes: data.len(),
            })?;
        let bit = (byte >> (7usize.saturating_sub(bit_in_byte))) & 1;
        value = value.checked_shl(1).ok_or(DecodeError::InvalidSampleData)?;
        value |= u32::from(bit);
    }

    Ok(value)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn decode_contiguous_1_bit_samples() {
        let samples = decode_sample_codes(
            &[0b1011_0010],
            1,
            SampleLayout::Contiguous { sample_count: 8 },
        )
        .unwrap();

        assert_eq!(samples, vec![1, 0, 1, 1, 0, 0, 1, 0]);
    }

    #[test]
    fn decode_contiguous_12_bit_samples() {
        let samples = decode_sample_codes(
            &[0xAB, 0xCD, 0x12],
            12,
            SampleLayout::Contiguous { sample_count: 2 },
        )
        .unwrap();

        assert_eq!(samples, vec![0xABC, 0xD12]);
    }

    #[test]
    fn decode_contiguous_wide_samples() {
        let samples = decode_sample_codes(
            &[0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE],
            24,
            SampleLayout::Contiguous { sample_count: 2 },
        )
        .unwrap();

        assert_eq!(samples, vec![0x12_34_56, 0x78_9A_BC]);
    }

    #[test]
    fn decode_contiguous_32_bit_samples() {
        let samples = decode_sample_codes(
            &[0x12, 0x34, 0x56, 0x78],
            32,
            SampleLayout::Contiguous { sample_count: 1 },
        )
        .unwrap();

        assert_eq!(samples, vec![0x12_34_56_78]);
    }

    #[test]
    fn decode_row_aligned_respects_padding() {
        let samples = decode_sample_codes(
            &[0b1010_0000, 0b0110_0000],
            1,
            SampleLayout::RowAligned {
                width: 3,
                height: 2,
                samples_per_pixel: 1,
            },
        )
        .unwrap();

        assert_eq!(samples, vec![1, 0, 1, 0, 1, 1]);
    }

    #[test]
    fn decode_row_aligned_multi_component_samples() {
        let samples = decode_sample_codes(
            &[0b1101_1000],
            1,
            SampleLayout::RowAligned {
                width: 2,
                height: 1,
                samples_per_pixel: 3,
            },
        )
        .unwrap();

        assert_eq!(samples, vec![1, 1, 0, 1, 1, 0]);
    }

    #[test]
    fn decode_reports_truncated_data() {
        let err = decode_sample_codes(&[0xFF], 16, SampleLayout::Contiguous { sample_count: 1 })
            .expect_err("16-bit sample requires two bytes");

        assert!(matches!(
            err,
            DecodeError::InsufficientData {
                expected_bytes: 2,
                actual_bytes: 1
            }
        ));
    }

    #[test]
    fn decode_normalizes_samples() {
        let samples = decode_normalized_samples(&[0x00, 0xFF], 8, 2).unwrap();

        assert_eq!(samples, vec![0.0, 1.0]);
    }
}
