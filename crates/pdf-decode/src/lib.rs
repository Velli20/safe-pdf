use num_traits::ToPrimitive;
use pdf_object::{
    dictionary::Dictionary, error::ObjectError, object_resolver::ObjectResolver,
    object_variant::ObjectVariant,
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DecodeError {
    #[error("{0}")]
    Object(#[from] ObjectError),
    #[error("unsupported bits per sample value: {bits_per_sample}")]
    InvalidBitsPerSample { bits_per_sample: usize },
    #[error(
        "sample data is truncated: expected at least {expected_bytes} bytes, got {actual_bytes}"
    )]
    InsufficientData {
        expected_bytes: usize,
        actual_bytes: usize,
    },
    #[error("invalid /Decode array length: expected {expected_values} values, got {actual_values}")]
    InvalidDecodeLength {
        expected_values: usize,
        actual_values: usize,
    },
    #[error("invalid /Decode value")]
    InvalidDecodeValue,
    #[error("palette base component count must be non-zero")]
    InvalidComponentCount,
    #[error(
        "palette index {index} out of bounds at pixel {pixel_index} (lookup table size: {lookup_len})"
    )]
    PaletteLookupOutOfBounds {
        index: u8,
        pixel_index: usize,
        lookup_len: usize,
    },
    #[error("sample data conversion failed")]
    InvalidSampleData,
}

#[derive(Debug, Clone, Copy)]
pub enum SampleLayout {
    Contiguous {
        sample_count: usize,
    },
    RowAligned {
        width: usize,
        height: usize,
        samples_per_pixel: usize,
    },
}

#[derive(Debug, Clone, Copy)]
pub struct DecodeRange {
    min: f32,
    max: f32,
}

impl DecodeRange {
    pub fn new(min: f32, max: f32) -> Result<Self, DecodeError> {
        if !min.is_finite() || !max.is_finite() {
            return Err(DecodeError::InvalidDecodeValue);
        }

        Ok(Self { min, max })
    }

    pub fn identity() -> Self {
        Self { min: 0.0, max: 1.0 }
    }

    pub fn inverted_identity() -> Self {
        Self { min: 1.0, max: 0.0 }
    }

    fn map_byte(&self, sample: u8, sample_max: u8, output_max: u8) -> u8 {
        let sample_max = f32::from(sample_max.max(1));
        let normalized = f32::from(sample) / sample_max;
        let decoded = self.min + normalized * (self.max - self.min);
        let scaled = decoded * f32::from(output_max);
        let clamped = scaled.clamp(0.0, f32::from(output_max));
        clamped.round().to_u8().unwrap_or(u8::MAX)
    }
}

#[derive(Debug, Clone)]
pub struct DecodeMap {
    ranges: Vec<DecodeRange>,
}

impl DecodeMap {
    pub fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
        component_count: usize,
        default_inverted: bool,
    ) -> Result<Self, DecodeError> {
        let default_range = if default_inverted {
            DecodeRange::inverted_identity()
        } else {
            DecodeRange::identity()
        };

        let ranges = match dictionary.get("Decode") {
            Some(value) => Self::parse_object(value, objects, component_count)?,
            None => vec![default_range; component_count],
        };

        Ok(Self { ranges })
    }

    pub fn parse_object(
        decode: &ObjectVariant,
        objects: &dyn ObjectResolver,
        component_count: usize,
    ) -> Result<Vec<DecodeRange>, DecodeError> {
        let values = decode.try_array(objects)?;
        let expected_values = component_count.saturating_mul(2);
        if values.len() != expected_values {
            return Err(DecodeError::InvalidDecodeLength {
                expected_values,
                actual_values: values.len(),
            });
        }

        let mut ranges = Vec::with_capacity(component_count);
        for pair in values.chunks_exact(2) {
            let [min, max] = pair else {
                return Err(DecodeError::InvalidDecodeLength {
                    expected_values,
                    actual_values: values.len(),
                });
            };
            ranges.push(DecodeRange::new(
                min.try_number::<f32>(objects)?,
                max.try_number::<f32>(objects)?,
            )?);
        }

        Ok(ranges)
    }

    pub fn apply_to_bytes(&self, samples: &[u8], sample_max: u8, output_max: u8) -> Vec<u8> {
        if self.ranges.is_empty() {
            return Vec::new();
        }

        let mut out = Vec::with_capacity(samples.len());
        for (sample, range) in samples.iter().zip(self.ranges.iter().cycle()) {
            out.push(range.map_byte(*sample, sample_max, output_max));
        }
        out
    }
}

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

pub fn expand_indexed_values(
    indices: &[u8],
    lookup: &[u8],
    hival: u8,
    base_components: usize,
) -> Result<Vec<u8>, DecodeError> {
    if base_components == 0 {
        return Err(DecodeError::InvalidComponentCount);
    }

    let mut out = Vec::with_capacity(indices.len().saturating_mul(base_components));
    for (pixel_index, &index) in indices.iter().enumerate() {
        let clamped_index = index.min(hival);
        let start = usize::from(clamped_index).saturating_mul(base_components);
        let end = start.saturating_add(base_components);
        let entry =
            lookup
                .get(start..end)
                .ok_or_else(|| DecodeError::PaletteLookupOutOfBounds {
                    index: clamped_index,
                    pixel_index,
                    lookup_len: lookup.len(),
                })?;
        out.extend_from_slice(entry);
    }

    Ok(out)
}

fn validate_bits_per_sample(bits_per_sample: usize) -> Result<(), DecodeError> {
    if matches!(bits_per_sample, 1 | 2 | 4 | 8 | 12 | 16 | 24 | 32) {
        return Ok(());
    }

    Err(DecodeError::InvalidBitsPerSample { bits_per_sample })
}

fn ensure_len(data: &[u8], expected_bytes: usize) -> Result<(), DecodeError> {
    if data.len() < expected_bytes {
        return Err(DecodeError::InsufficientData {
            expected_bytes,
            actual_bytes: data.len(),
        });
    }

    Ok(())
}

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
    use std::collections::BTreeMap;

    use pdf_object::{dictionary::Dictionary, object_resolver::PassthroughResolver};

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

    #[test]
    fn decode_map_defaults_to_identity() {
        let dictionary = Dictionary::new(BTreeMap::new());
        let map = DecodeMap::from_dictionary(&dictionary, &PassthroughResolver, 2, false).unwrap();
        let out = map.apply_to_bytes(&[0, 255, 128, 64], 255, 255);

        assert_eq!(out, vec![0, 255, 128, 64]);
    }

    #[test]
    fn decode_map_supports_inverted_default() {
        let dictionary = Dictionary::new(BTreeMap::new());
        let map = DecodeMap::from_dictionary(&dictionary, &PassthroughResolver, 1, true).unwrap();
        let out = map.apply_to_bytes(&[0, 1], 1, 255);

        assert_eq!(out, vec![255, 0]);
    }

    #[test]
    fn decode_map_rejects_invalid_length() {
        let err = DecodeMap::from_dictionary(
            &Dictionary::new(BTreeMap::from([(
                "Decode".to_string(),
                ObjectVariant::Array(vec![ObjectVariant::Integer(0)]),
            )])),
            &PassthroughResolver,
            1,
            false,
        )
        .expect_err("invalid decode length should fail");

        assert!(matches!(
            err,
            DecodeError::InvalidDecodeLength {
                expected_values: 2,
                actual_values: 1
            }
        ));
    }

    #[test]
    fn decode_map_rejects_non_finite_values() {
        let err = DecodeMap::from_dictionary(
            &Dictionary::new(BTreeMap::from([(
                "Decode".to_string(),
                ObjectVariant::Array(vec![
                    ObjectVariant::Real(f64::NAN),
                    ObjectVariant::Integer(1),
                ]),
            )])),
            &PassthroughResolver,
            1,
            false,
        )
        .expect_err("nan decode values should fail");

        assert!(matches!(err, DecodeError::InvalidDecodeValue));
    }

    #[test]
    fn decode_map_cycles_ranges_per_component() {
        let dictionary = Dictionary::new(BTreeMap::from([(
            "Decode".to_string(),
            ObjectVariant::Array(vec![
                ObjectVariant::Integer(0),
                ObjectVariant::Real(0.5),
                ObjectVariant::Integer(1),
                ObjectVariant::Integer(0),
            ]),
        )]));
        let map = DecodeMap::from_dictionary(&dictionary, &PassthroughResolver, 2, false).unwrap();
        let out = map.apply_to_bytes(&[255, 0, 0, 255], 255, 255);

        assert_eq!(out, vec![128, 255, 0, 0]);
    }

    #[test]
    fn expand_indexed_values_supports_clamping() {
        let out = expand_indexed_values(&[2], &[10, 11, 12, 20, 21, 22], 1, 3).unwrap();

        assert_eq!(out, vec![20, 21, 22]);
    }

    #[test]
    fn expand_indexed_values_rejects_zero_components() {
        let err = expand_indexed_values(&[0], &[10], 0, 0)
            .expect_err("zero component palettes should fail");

        assert!(matches!(err, DecodeError::InvalidComponentCount));
    }

    #[test]
    fn expand_indexed_values_rejects_short_lookup() {
        let err = expand_indexed_values(&[1], &[10, 11, 12], 1, 3)
            .expect_err("lookup is too short for palette index 1");

        assert!(matches!(
            err,
            DecodeError::PaletteLookupOutOfBounds {
                index: 1,
                pixel_index: 0,
                lookup_len: 3
            }
        ));
    }
}
