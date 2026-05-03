use num_traits::ToPrimitive;
use pdf_object::{
    dictionary::Dictionary, object_resolver::ObjectResolver, object_variant::ObjectVariant,
};

use crate::PdfImageError;

#[derive(Debug, Clone, Copy)]
struct DecodeRange {
    min: f32,
    max: f32,
}

impl DecodeRange {
    fn new(min: f32, max: f32) -> Result<Self, PdfImageError> {
        if !min.is_finite() || !max.is_finite() {
            return Err(PdfImageError::InvalidDecodeValue);
        }

        Ok(Self { min, max })
    }

    fn default() -> Self {
        Self { min: 0.0, max: 1.0 }
    }

    fn inverted_default() -> Self {
        Self { min: 1.0, max: 0.0 }
    }

    fn map_sample(&self, sample: u8, sample_max: u8, output_max: u8) -> u8 {
        let sample_max = f32::from(sample_max.max(1));
        let normalized = f32::from(sample) / sample_max;
        let decoded = self.min + normalized * (self.max - self.min);
        let scaled = decoded * f32::from(output_max);
        let clamped = scaled.clamp(0.0, f32::from(output_max));
        clamped.round().to_u8().unwrap_or(u8::MAX)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ImageDecode {
    ranges: Vec<DecodeRange>,
    sample_max: u8,
    output_max: u8,
}

impl ImageDecode {
    pub(crate) fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
        component_count: usize,
        sample_max: u8,
        output_max: u8,
        default_inverted: bool,
    ) -> Result<Self, PdfImageError> {
        let default_range = if default_inverted {
            DecodeRange::inverted_default()
        } else {
            DecodeRange::default()
        };

        let ranges = match dictionary.get("Decode") {
            Some(decode) => Self::parse_ranges(decode, objects, component_count)?,
            None => vec![default_range; component_count],
        };

        Ok(Self {
            ranges,
            sample_max,
            output_max,
        })
    }

    fn parse_ranges(
        decode: &ObjectVariant,
        objects: &dyn ObjectResolver,
        component_count: usize,
    ) -> Result<Vec<DecodeRange>, PdfImageError> {
        let values = decode.try_array(objects)?;
        let expected_values = component_count.saturating_mul(2);
        if values.len() != expected_values {
            return Err(PdfImageError::InvalidDecodeLength {
                expected_values,
                actual_values: values.len(),
            });
        }

        let mut ranges = Vec::with_capacity(component_count);
        for pair in values.chunks_exact(2) {
            let [min, max] = pair else { unreachable!() };
            ranges.push(DecodeRange::new(
                min.try_number::<f32>(objects)?,
                max.try_number::<f32>(objects)?,
            )?);
        }

        Ok(ranges)
    }

    pub(crate) fn apply(&self, samples: &[u8]) -> Vec<u8> {
        if self.ranges.is_empty() {
            return Vec::new();
        }

        let mut out = Vec::with_capacity(samples.len());
        for (sample, range) in samples.iter().zip(self.ranges.iter().cycle()) {
            out.push(range.map_sample(*sample, self.sample_max, self.output_max));
        }
        out
    }
}
