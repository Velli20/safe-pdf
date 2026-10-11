//! PDF `/Decode` map parsing and application helpers.

use crate::{error::DecodeError, range::DecodeRange};

/// Stores the per-component decode ranges used to transform packed samples.
#[derive(Debug, Clone, PartialEq)]
pub struct DecodeMap {
    ranges: Vec<DecodeRange>,
}

impl DecodeMap {
    /// Builds a decode map from the values of a `/Decode` array.
    ///
    /// Values beyond the two needed per component are ignored.
    pub fn new(values: &[f32], component_count: usize) -> Result<Self, DecodeError> {
        let expected_values = component_count.saturating_mul(2);
        let Some(values) = values.get(..expected_values) else {
            return Err(DecodeError::InvalidDecodeLength {
                expected_values,
                actual_values: values.len(),
            });
        };

        let ranges = values
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&[min, max]| DecodeRange::new(min, max))
            .collect::<Result<_, _>>()?;

        Ok(Self { ranges })
    }

    /// Applies the decode map to packed sample bytes.
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

    /// Applies the decode ranges to `samples`, writing their floating-point component values
    /// into `out`. Writing stops at the shorter of the two slices.
    pub fn apply_to_f32(&self, samples: &[u8], sample_max: u8, out: &mut [f32]) {
        for ((sample, range), value) in samples
            .iter()
            .zip(self.ranges.iter().cycle())
            .zip(out.iter_mut())
        {
            *value = range.map_f32(*sample, sample_max);
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn decode_map_rejects_invalid_length() {
        let err = DecodeMap::new(&[0.0], 1).expect_err("invalid decode length should fail");

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
        let err = DecodeMap::new(&[f32::NAN, 1.0], 1).expect_err("nan decode values should fail");

        assert!(matches!(err, DecodeError::InvalidDecodeValue));
    }

    #[test]
    fn decode_map_cycles_ranges_per_component() {
        let map = DecodeMap::new(&[0.0, 0.5, 1.0, 0.0], 2).unwrap();
        let out = map.apply_to_bytes(&[255, 0, 0, 255], 255, 255);

        assert_eq!(out, vec![128, 255, 0, 0]);
    }

    #[test]
    fn decode_map_preserves_non_normalized_float_ranges() {
        let map = DecodeMap {
            ranges: vec![DecodeRange::new(-100.0, 100.0).unwrap()],
        };

        let mut values = [0.0; 2];
        map.apply_to_f32(&[0, 255], 255, &mut values);
        assert_eq!(values, [-100.0, 100.0]);
    }
}
