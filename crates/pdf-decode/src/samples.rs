//! Sample code unpacking and normalization helpers.

use std::borrow::Cow;

use num_traits::ToPrimitive;
use pdf_utils::BitReader;

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
            decode_contiguous_samples(data, bits_per_sample, sample_count)
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
            decode_row_aligned_samples(
                data,
                bits_per_sample,
                width,
                height,
                samples_per_pixel,
                bytes_per_row,
                total_samples,
            )
        }
    }
}

/// Decodes packed sample codes into byte-sized sample values.
///
/// Eight-bit samples borrow directly from the input. Other sample sizes are
/// decoded into an owned buffer and return an error when a value cannot fit in
/// a byte.
pub fn decode_sample_bytes<'a>(
    data: &'a [u8],
    bits_per_sample: usize,
    layout: SampleLayout,
) -> Result<Cow<'a, [u8]>, DecodeError> {
    validate_bits_per_sample(bits_per_sample)?;

    if bits_per_sample == 8 {
        let sample_count = match layout {
            SampleLayout::Contiguous { sample_count } => sample_count,
            SampleLayout::RowAligned {
                width,
                height,
                samples_per_pixel,
            } => width
                .saturating_mul(height)
                .saturating_mul(samples_per_pixel),
        };
        let samples = data
            .get(..sample_count)
            .ok_or(DecodeError::InsufficientData {
                expected_bytes: sample_count,
                actual_bytes: data.len(),
            })?;
        return Ok(Cow::Borrowed(samples));
    }

    let samples = decode_sample_codes(data, bits_per_sample, layout)?
        .into_iter()
        .map(|sample| u8::try_from(sample).map_err(|_| DecodeError::InvalidSampleData))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Cow::Owned(samples))
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

fn decode_contiguous_samples(
    data: &[u8],
    bits_per_sample: usize,
    sample_count: usize,
) -> Result<Vec<u32>, DecodeError> {
    match bits_per_sample {
        8 => Ok(data
            .iter()
            .take(sample_count)
            .map(|byte| u32::from(*byte))
            .collect()),
        16 => decode_aligned_samples(data, sample_count, 2, |chunk| match chunk {
            [first, second] => u32::from(u16::from_be_bytes([*first, *second])),
            _ => 0,
        }),
        24 => decode_aligned_samples(data, sample_count, 3, |chunk| {
            let first = chunk.first().copied().unwrap_or_default();
            let second = chunk.get(1).copied().unwrap_or_default();
            let third = chunk.get(2).copied().unwrap_or_default();
            (u32::from(first) << 16) | (u32::from(second) << 8) | u32::from(third)
        }),
        32 => decode_aligned_samples(data, sample_count, 4, |chunk| match chunk {
            [first, second, third, fourth] => {
                u32::from_be_bytes([*first, *second, *third, *fourth])
            }
            _ => 0,
        }),
        _ => decode_packed_samples(data, bits_per_sample, sample_count),
    }
}

fn decode_row_aligned_samples(
    data: &[u8],
    bits_per_sample: usize,
    width: usize,
    height: usize,
    samples_per_pixel: usize,
    bytes_per_row: usize,
    total_samples: usize,
) -> Result<Vec<u32>, DecodeError> {
    let samples_per_row = width.saturating_mul(samples_per_pixel);

    if bits_per_sample == 8 {
        return Ok(data
            .iter()
            .take(total_samples)
            .map(|byte| u32::from(*byte))
            .collect());
    }

    let bytes_per_sample = bits_per_sample / 8;
    let is_byte_aligned = bytes_per_sample.saturating_mul(8) == bits_per_sample;

    let mut out = Vec::with_capacity(total_samples);
    for row in 0..height {
        let row_start = row.saturating_mul(bytes_per_row);
        let row_end = row_start.saturating_add(bytes_per_row);
        let row_data = data
            .get(row_start..row_end)
            .ok_or(DecodeError::InsufficientData {
                expected_bytes: row_end,
                actual_bytes: data.len(),
            })?;

        if is_byte_aligned {
            let sample_bytes = samples_per_row.saturating_mul(bytes_per_sample);
            let packed_row =
                row_data
                    .get(..sample_bytes)
                    .ok_or_else(|| DecodeError::InsufficientData {
                        expected_bytes: row_start.saturating_add(sample_bytes),
                        actual_bytes: data.len(),
                    })?;
            out.extend(decode_contiguous_samples(
                packed_row,
                bits_per_sample,
                samples_per_row,
            )?);
        } else {
            out.extend(decode_packed_samples(
                row_data,
                bits_per_sample,
                samples_per_row,
            )?);
        }
    }

    Ok(out)
}

fn decode_aligned_samples<F>(
    data: &[u8],
    sample_count: usize,
    bytes_per_sample: usize,
    decode_chunk: F,
) -> Result<Vec<u32>, DecodeError>
where
    F: Fn(&[u8]) -> u32,
{
    let mut out = Vec::with_capacity(sample_count);
    for chunk in data.chunks_exact(bytes_per_sample).take(sample_count) {
        out.push(decode_chunk(chunk));
    }
    Ok(out)
}

fn decode_packed_samples(
    data: &[u8],
    bits_per_sample: usize,
    sample_count: usize,
) -> Result<Vec<u32>, DecodeError> {
    let total_bits = sample_count.saturating_mul(bits_per_sample);
    let required_bytes = total_bits.div_ceil(8);
    ensure_len(data, required_bytes)?;
    let bit_width = u8::try_from(bits_per_sample)
        .map_err(|_| DecodeError::InvalidBitsPerSample { bits_per_sample })?;

    let mut reader = BitReader::new(data);
    let mut out = Vec::with_capacity(sample_count);
    for _ in 0..sample_count {
        let sample = reader
            .read_bits_u32(bit_width)
            .ok_or(DecodeError::InsufficientData {
                expected_bytes: required_bytes,
                actual_bytes: data.len(),
            })?;
        out.push(sample);
    }
    Ok(out)
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
    fn decode_contiguous_2_and_4_bit_samples() {
        let two_bit_samples = decode_sample_codes(
            &[0b00_01_10_11],
            2,
            SampleLayout::Contiguous { sample_count: 4 },
        )
        .unwrap();
        let four_bit_samples = decode_sample_codes(
            &[0x1F, 0xA0],
            4,
            SampleLayout::Contiguous { sample_count: 3 },
        )
        .unwrap();

        assert_eq!(two_bit_samples, vec![0, 1, 2, 3]);
        assert_eq!(four_bit_samples, vec![1, 15, 10]);
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
    fn decode_contiguous_8_bit_samples_fast_path() {
        let samples = decode_sample_codes(
            &[0x12, 0x34, 0x56],
            8,
            SampleLayout::Contiguous { sample_count: 3 },
        )
        .unwrap();

        assert_eq!(samples, vec![0x12, 0x34, 0x56]);
    }

    #[test]
    fn decode_row_aligned_8_bit_samples_fast_path() {
        let samples = decode_sample_codes(
            &[0x10, 0x20, 0x30, 0x40],
            8,
            SampleLayout::RowAligned {
                width: 2,
                height: 2,
                samples_per_pixel: 1,
            },
        )
        .unwrap();

        assert_eq!(samples, vec![0x10, 0x20, 0x30, 0x40]);
    }

    #[test]
    fn decode_contiguous_8_bit_sample_bytes() {
        let data = [0x12, 0x34, 0x56, 0x78];
        let samples =
            decode_sample_bytes(&data, 8, SampleLayout::Contiguous { sample_count: 3 }).unwrap();

        assert!(matches!(samples, Cow::Borrowed(_)));
        assert_eq!(samples.as_ref(), &data[..3]);
    }

    #[test]
    fn decode_row_aligned_8_bit_sample_bytes() {
        let samples = decode_sample_bytes(
            &[0x10, 0x20, 0x30, 0x40],
            8,
            SampleLayout::RowAligned {
                width: 1,
                height: 2,
                samples_per_pixel: 2,
            },
        )
        .unwrap();

        assert!(matches!(samples, Cow::Borrowed(_)));
        assert_eq!(samples.as_ref(), &[0x10, 0x20, 0x30, 0x40]);
    }

    #[test]
    fn decode_packed_sample_bytes_respect_row_padding() {
        let samples = decode_sample_bytes(
            &[0b1010_0000, 0b0110_0000],
            1,
            SampleLayout::RowAligned {
                width: 3,
                height: 2,
                samples_per_pixel: 1,
            },
        )
        .unwrap();

        assert!(matches!(samples, Cow::Owned(_)));
        assert_eq!(samples.as_ref(), &[1, 0, 1, 0, 1, 1]);
    }

    #[test]
    fn decode_sample_bytes_reports_truncated_8_bit_data() {
        let err = decode_sample_bytes(
            &[0x10, 0x20, 0x30],
            8,
            SampleLayout::RowAligned {
                width: 2,
                height: 2,
                samples_per_pixel: 1,
            },
        )
        .expect_err("four byte samples require four input bytes");

        assert!(matches!(
            err,
            DecodeError::InsufficientData {
                expected_bytes: 4,
                actual_bytes: 3
            }
        ));
    }

    #[test]
    fn decode_sample_bytes_rejects_wide_values() {
        let err = decode_sample_bytes(
            &[0xAB, 0xC0],
            12,
            SampleLayout::Contiguous { sample_count: 1 },
        )
        .expect_err("12-bit sample does not fit in a byte");

        assert!(matches!(err, DecodeError::InvalidSampleData));
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
    fn decode_reports_truncated_packed_data() {
        let err = decode_sample_codes(&[0xFF], 12, SampleLayout::Contiguous { sample_count: 1 })
            .expect_err("12-bit sample requires two bytes");

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
