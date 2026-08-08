//! PDF `/Decode` map parsing and application helpers.

use pdf_object::{
    dictionary::Dictionary, object_resolver::ObjectResolver, object_variant::ObjectVariant,
};

use crate::{error::DecodeError, range::DecodeRange};

/// Stores the per-component decode ranges used to transform packed samples.
#[derive(Debug, Clone)]
pub struct DecodeMap {
    ranges: Vec<DecodeRange>,
}

impl DecodeMap {
    /// Builds a decode map from a PDF dictionary and object resolver.
    ///
    /// Returns `None` when the dictionary does not contain a `/Decode` entry.
    pub fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
        component_count: usize,
    ) -> Result<Option<Self>, DecodeError> {
        let Some(value) = dictionary.get("Decode") else {
            return Ok(None);
        };
        let ranges = Self::parse_object(value, objects, component_count)?;

        Ok(Some(Self { ranges }))
    }

    /// Parses a `/Decode` array into per-component ranges.
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
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::collections::BTreeMap;

    use pdf_object::{
        dictionary::Dictionary, object_resolver::PassthroughResolver, object_variant::ObjectVariant,
    };

    use super::*;

    #[test]
    fn decode_map_is_absent_without_decode_entry() {
        let dictionary = Dictionary::new(BTreeMap::new());
        let map = DecodeMap::from_dictionary(&dictionary, &PassthroughResolver, 2).unwrap();

        assert!(map.is_none());
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
        let map = DecodeMap::from_dictionary(&dictionary, &PassthroughResolver, 2)
            .unwrap()
            .expect("explicit /Decode should create a map");
        let out = map.apply_to_bytes(&[255, 0, 0, 255], 255, 255);

        assert_eq!(out, vec![128, 255, 0, 0]);
    }
}
