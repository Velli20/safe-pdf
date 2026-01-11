use num_derive::FromPrimitive;
use num_traits::{FromPrimitive, ToPrimitive};
use pdf_object::{ObjectVariant, object_collection::ObjectCollection};

use crate::functions::{
    Function, FunctionImpl, FunctionInterpolationError, FunctionReadError, ensure_stream_len,
    get_pair, linear_interpolate,
};

#[derive(Debug, Clone, Copy, PartialEq, FromPrimitive, Default)]
enum InterpolationOrder {
    #[default]
    Linear = 1,
    Cubic = 3,
}

#[derive(Debug, Clone)]
pub struct SampledFunction {
    /// Number of samples in each input dimension.
    size: Vec<usize>,
    /// Number of bits per sample value (stored for debugging/introspection).
    #[allow(dead_code)]
    bits_per_sample: usize,
    /// Interpolation order.
    order: InterpolationOrder,
    /// Input encoding: maps domain to sample indices.
    encode: Vec<f32>,
    /// Output decoding: maps sample values to output range.
    decode: Vec<f32>,
    /// Input domain as pairs `[min0, max0, min1, max1, ...]`.
    domain: Vec<f32>,
    /// Output range as pairs `[min0, max0, min1, max1, ...]`.
    range: Vec<f32>,
    /// The decoded sample values, stored as f32 for efficiency.
    samples: Vec<f32>,
    /// Number of output values per sample point.
    output_count: usize,
}

impl SampledFunction {
    /// Reads a value of the specified bit width from a byte stream at a bit offset.
    #[inline]
    fn read_bits(data: &[u8], bit_offset: usize, bits: usize) -> u32 {
        if bits == 0 {
            return 0;
        }

        let byte_offset = bit_offset / 8;
        let bit_shift = bit_offset % 8;

        let total_bits = bit_shift.saturating_add(bits);
        let bytes_needed = total_bits.div_ceil(8);

        // Read enough bytes to cover our bits (up to 5 bytes for 32-bit at worst alignment)
        let mut value: u64 = 0;
        for i in 0..bytes_needed {
            let Some(&byte) = data.get(byte_offset.saturating_add(i)) else {
                break;
            };

            let shift_bytes = bytes_needed.saturating_sub(1).saturating_sub(i);
            let shift_bits = shift_bytes.saturating_mul(8);
            value |= u64::from(byte) << shift_bits;
        }

        // Shift and mask to extract the value
        let shift = bytes_needed
            .saturating_mul(8)
            .saturating_sub(bit_shift)
            .saturating_sub(bits);
        let bits_u32 = u32::try_from(bits).unwrap_or(u64::BITS);
        let mask = u64::MAX
            .checked_shr(u64::BITS.saturating_sub(bits_u32))
            .unwrap_or(0);

        u32::try_from((value >> shift) & mask).unwrap_or(0)
    }

    /// Decodes sample values from the stream data.
    ///
    /// Samples are packed into the stream with the specified bits per sample.
    /// Returns normalized values in the range [0, 1].
    fn decode_samples(
        stream: &[u8],
        bits_per_sample: usize,
        count: usize,
    ) -> Result<Vec<f32>, FunctionReadError> {
        let max_value = (1u64 << bits_per_sample).saturating_sub(1);
        let max_value_f32 = max_value.to_f32().unwrap_or(f32::MAX);

        let mut samples = Vec::with_capacity(count);

        match bits_per_sample {
            8 => {
                // Fast path for 8-bit samples
                ensure_stream_len(stream, count)?;
                for &byte in stream.iter().take(count) {
                    samples.push(f32::from(byte) / max_value_f32);
                }
            }
            16 => {
                // Fast path for 16-bit samples (big-endian)
                let expected_bytes = count.saturating_mul(2);
                ensure_stream_len(stream, expected_bytes)?;
                for chunk in stream.chunks_exact(2).take(count) {
                    // chunks_exact guarantees exactly 2 elements
                    if let [b0, b1] = chunk {
                        let value = u16::from_be_bytes([*b0, *b1]);
                        samples.push(f32::from(value) / max_value_f32);
                    }
                }
            }
            24 => {
                // 24-bit samples (big-endian)
                let expected_bytes = count.saturating_mul(3);
                ensure_stream_len(stream, expected_bytes)?;
                for chunk in stream.chunks_exact(3).take(count) {
                    // chunks_exact guarantees exactly 3 elements
                    if let [b0, b1, b2] = chunk {
                        let value = u32::from_be_bytes([0, *b0, *b1, *b2]);
                        samples.push(value.to_f32().unwrap_or(f32::MAX) / max_value_f32);
                    }
                }
            }
            32 => {
                // 32-bit samples (big-endian)
                let expected_bytes = count.saturating_mul(4);
                ensure_stream_len(stream, expected_bytes)?;
                for chunk in stream.chunks_exact(4).take(count) {
                    // chunks_exact guarantees exactly 4 elements
                    if let [b0, b1, b2, b3] = chunk {
                        let value = u32::from_be_bytes([*b0, *b1, *b2, *b3]);
                        samples.push(value.to_f32().unwrap_or(f32::MAX) / max_value_f32);
                    }
                }
            }
            _ => {
                // Generic bit-packed decoding for 1, 2, 4, 12 bits
                let total_bits = count.saturating_mul(bits_per_sample);
                let expected_bytes = total_bits.div_ceil(8);
                ensure_stream_len(stream, expected_bytes)?;

                let mut bit_offset: usize = 0;
                for _ in 0..count {
                    let value = Self::read_bits(stream, bit_offset, bits_per_sample);
                    samples.push(value.to_f32().unwrap_or(f32::MAX) / max_value_f32);
                    bit_offset = bit_offset.saturating_add(bits_per_sample);
                }
            }
        }

        Ok(samples)
    }
}

impl FunctionImpl for SampledFunction {
    /// Interpolates using a sampled function (Type 0).
    ///
    /// This implementation supports 1-dimensional input with linear interpolation.
    /// Multi-dimensional input and cubic interpolation are handled with fallback behavior.
    fn interpolate(&self, x: f32) -> Result<Vec<f32>, FunctionInterpolationError> {
        // For simplicity, we focus on 1-dimensional input (most common case)
        // The algorithm can be extended for multi-dimensional input if needed

        // Clamp input to domain
        let (domain_min, domain_max) =
            get_pair(&self.domain, 0).ok_or(FunctionInterpolationError::EncodeIndexError)?;
        let x_clamped = x.clamp(domain_min, domain_max);

        // Map input to sample index using encode array
        let (encode_min, encode_max) =
            get_pair(&self.encode, 0).ok_or(FunctionInterpolationError::EncodeIndexError)?;

        // Linear interpolation from [domain_min, domain_max] to [encode_min, encode_max]
        let encoded = linear_interpolate(x_clamped, domain_min, domain_max, encode_min, encode_max);

        // Clamp to valid sample index range
        let size_0 = self.size.first().copied().unwrap_or(1);
        let max_index_u32 = u32::try_from(size_0.saturating_sub(1)).unwrap_or(u32::MAX);
        let max_index = max_index_u32.to_f32().unwrap_or(f32::MAX);
        let sample_index = encoded.clamp(0.0, max_index);

        // Get the integer index and fractional part for interpolation
        // Safety: floor() of a clamped non-negative float is always a valid usize
        let index_low_f32 = sample_index.floor();
        let index_high_f32 = (index_low_f32 + 1.0).min(max_index);
        let frac = sample_index.fract();

        // Compute output values
        let mut outputs = Vec::with_capacity(self.output_count);

        // `sample_index` is clamped into [0, max_index], so these casts are safe.
        let index_low = index_low_f32
            .to_usize()
            .ok_or(FunctionInterpolationError::SampleIndexOutOfBounds)?;
        let index_high = index_high_f32
            .to_usize()
            .ok_or(FunctionInterpolationError::SampleIndexOutOfBounds)?;

        for j in 0..self.output_count {
            // Get sample values at low and high indices
            let low_idx = index_low
                .checked_mul(self.output_count)
                .and_then(|v| v.checked_add(j))
                .ok_or(FunctionInterpolationError::SampleIndexOutOfBounds)?;
            let sample_low = self
                .samples
                .get(low_idx)
                .copied()
                .ok_or(FunctionInterpolationError::SampleIndexOutOfBounds)?;

            let high_idx = index_high
                .checked_mul(self.output_count)
                .and_then(|v| v.checked_add(j))
                .ok_or(FunctionInterpolationError::SampleIndexOutOfBounds)?;
            let sample_high = self
                .samples
                .get(high_idx)
                .copied()
                .ok_or(FunctionInterpolationError::SampleIndexOutOfBounds)?;

            // Interpolate between samples
            let interpolated =
                if self.order == InterpolationOrder::Linear || index_low == index_high {
                    // Linear interpolation (or no interpolation needed)
                    sample_low + frac * (sample_high - sample_low)
                } else {
                    // For cubic interpolation, fall back to linear for simplicity
                    // A full cubic spline implementation would require more samples
                    sample_low + frac * (sample_high - sample_low)
                };

            // Apply decode mapping: map from [0, 1] to [decode_min, decode_max]
            let (decode_min, decode_max) =
                get_pair(&self.decode, j).ok_or(FunctionInterpolationError::EncodeIndexError)?;
            let decoded = decode_min + interpolated * (decode_max - decode_min);

            // Clamp to range
            let (range_min, range_max) =
                get_pair(&self.range, j).ok_or(FunctionInterpolationError::EncodeIndexError)?;
            outputs.push(decoded.clamp(range_min, range_max));
        }

        Ok(outputs)
    }

    fn domain(&self) -> Option<[f32; 2]> {
        let first = *self.domain.first()?;
        let second = *self.domain.get(1)?;
        Some([first, second])
    }

    /// Parses a Type 0 (Sampled) function.
    ///
    /// Sampled functions use a lookup table of sample values with optional
    /// linear or cubic spline interpolation between samples.
    fn parse(
        object: &ObjectVariant,
        objects: &ObjectCollection,
    ) -> Result<Function, FunctionReadError> {
        let stream = objects.resolve_stream(object)?;
        let dictionary = &stream.dictionary;

        // /Domain: Required. Array of 2*m numbers defining input domain.
        let domain = dictionary.get_or_err("Domain")?.as_vec_of::<f32>()?;

        // /Range: Required for sampled functions. Array of 2*n numbers.
        let range = dictionary.get_or_err("Range")?.as_vec_of::<f32>()?;

        // /Size: Required. Array of m integers specifying samples per input dimension.
        let size = dictionary.get_or_err("Size")?.as_vec_of::<usize>()?;
        if size.is_empty() {
            return Err(FunctionReadError::InvalidSizeArray);
        }

        let output_count = range.len() / 2;

        // /BitsPerSample: Required. Must be 1, 2, 4, 8, 12, 16, 24, or 32.
        let bits_per_sample = dictionary
            .get_or_err("BitsPerSample")?
            .as_number::<usize>()?;
        if !matches!(bits_per_sample, 1 | 2 | 4 | 8 | 12 | 16 | 24 | 32) {
            return Err(FunctionReadError::InvalidBitsPerSample);
        }

        // /Order: Optional. 1 = linear (default), 3 = cubic spline.
        let order = dictionary
            .get("Order")
            .map(|o| {
                InterpolationOrder::from_i32(o.as_number::<i32>()?)
                    .ok_or(FunctionReadError::InvalidOrder)
            })
            .transpose()?
            .unwrap_or_default();

        // /Encode: Optional. Defaults to [0, Size[0]-1, 0, Size[1]-1, ...].
        let encode = dictionary
            .get("Encode")
            .map(ObjectVariant::as_vec_of::<f32>)
            .transpose()?
            .unwrap_or_else(|| {
                size.iter()
                    .flat_map(|&s| {
                        let max_u32 = u32::try_from(s.saturating_sub(1)).unwrap_or(u32::MAX);
                        let max_f32 = max_u32.to_f32().unwrap_or(f32::MAX);
                        [0.0, max_f32]
                    })
                    .collect()
            });

        // /Decode: Optional. Defaults to Range values.
        let decode = dictionary
            .get("Decode")
            .map(ObjectVariant::as_vec_of::<f32>)
            .transpose()?
            .unwrap_or_else(|| range.clone());

        if decode.len() != output_count.saturating_mul(2) {
            return Err(FunctionReadError::InvalidDecodeLength);
        }

        // Calculate total number of samples
        let total_samples: usize = size.iter().try_fold(1usize, |acc, &dim| {
            acc.checked_mul(dim)
                .ok_or(FunctionReadError::InvalidSizeArray)
        })?;
        let samples_count = total_samples
            .checked_mul(output_count)
            .ok_or(FunctionReadError::InvalidSizeArray)?;

        let stream = stream.data()?;

        // Decode samples from the stream
        let samples = Self::decode_samples(&stream, bits_per_sample, samples_count)?;

        Ok(Function::Sampled(SampledFunction {
            size,
            bits_per_sample,
            order,
            encode,
            decode,
            domain,
            range,
            samples,
            output_count,
        }))
    }
}
