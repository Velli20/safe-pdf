use num_derive::FromPrimitive;
use num_traits::{FromPrimitive, ToPrimitive};
use pdf_object::{object_resolver::ObjectResolver, object_variant::ObjectVariant};

use crate::function::{
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
    /// Input encoding: maps domain to sample indices, stored as pairs per dimension.
    encode: Vec<f32>,
    /// Output decoding: maps sample values to output range, stored as pairs per output.
    decode: Vec<f32>,
    /// Input domain as pairs `[min0, max0, min1, max1, ...]`.
    domain: Vec<f32>,
    /// Output range as pairs `[min0, max0, min1, max1, ...]`.
    range: Vec<f32>,
    /// The decoded sample values, stored as f32.
    /// Layout: samples[(i0 * size[1] * ... * size[m-1] + ... + i[m-1]) * output_count + j]
    samples: Vec<f32>,
    /// Number of output values per sample point.
    output_count: usize,
}

impl SampledFunction {
    /// Reads a value of the specified bit width from a byte stream at a bit offset.
    #[inline]
    fn read_bits(data: &[u8], bit_offset: usize, bits: usize) -> Result<u32, FunctionReadError> {
        if bits == 0 {
            return Ok(0);
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
        let bits_u32 = u32::try_from(bits).map_err(|_| FunctionReadError::InvalidBitsPerSample)?;
        let mask = u64::MAX
            .checked_shr(u64::BITS.saturating_sub(bits_u32))
            .ok_or(FunctionReadError::InvalidBitsPerSample)?;

        u32::try_from((value >> shift) & mask).map_err(|_| FunctionReadError::InvalidSampleData)
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
        let max_value_f32 = max_value
            .to_f32()
            .ok_or(FunctionReadError::InvalidSampleData)?;

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
                        let value_f32 =
                            value.to_f32().ok_or(FunctionReadError::InvalidSampleData)?;
                        samples.push(value_f32 / max_value_f32);
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
                        let value_f32 =
                            value.to_f32().ok_or(FunctionReadError::InvalidSampleData)?;
                        samples.push(value_f32 / max_value_f32);
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
                    let value = Self::read_bits(stream, bit_offset, bits_per_sample)?;
                    let value_f32 = value.to_f32().ok_or(FunctionReadError::InvalidSampleData)?;
                    samples.push(value_f32 / max_value_f32);
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
    /// Supports N-dimensional multilinear interpolation as defined in ISO 32000 §7.10.2.
    /// Cubic spline interpolation (Order=3) is recognised in the dictionary but not yet
    /// implemented; it returns [`FunctionInterpolationError::CubicInterpolationNotSupported`].
    fn interpolate(&self, inputs: &[f32]) -> Result<Vec<f32>, FunctionInterpolationError> {
        let m = self.size.len();

        if inputs.len() < m {
            return Err(FunctionInterpolationError::InsufficientInputs {
                expected: m,
                got: inputs.len(),
            });
        }

        if self.order == InterpolationOrder::Cubic {
            return Err(FunctionInterpolationError::CubicInterpolationNotSupported);
        }

        // Encode each input dimension to a continuous sample coordinate.
        let mut enc_coords: Vec<f32> = Vec::with_capacity(m);
        for i in 0..m {
            let (domain_min, domain_max) =
                get_pair(&self.domain, i).ok_or(FunctionInterpolationError::EncodeIndexError)?;
            let x_clamped = inputs
                .get(i)
                .copied()
                .ok_or(FunctionInterpolationError::InsufficientInputs {
                    expected: m,
                    got: inputs.len(),
                })?
                .clamp(domain_min, domain_max);

            let (encode_min, encode_max) =
                get_pair(&self.encode, i).ok_or(FunctionInterpolationError::EncodeIndexError)?;
            let encoded =
                linear_interpolate(x_clamped, domain_min, domain_max, encode_min, encode_max);

            let size_i = self
                .size
                .get(i)
                .copied()
                .ok_or(FunctionInterpolationError::EncodeIndexError)?;
            let max_i = size_i
                .saturating_sub(1)
                .to_f32()
                .ok_or(FunctionInterpolationError::SampleIndexOutOfBounds)?;

            enc_coords.push(encoded.clamp(0.0, max_i));
        }

        // Decompose each encoded coordinate into floor index, ceiling index, and fraction.
        let mut idx_low: Vec<usize> = Vec::with_capacity(m);
        let mut idx_high: Vec<usize> = Vec::with_capacity(m);
        let mut fracs: Vec<f32> = Vec::with_capacity(m);
        for (i, &enc) in enc_coords.iter().enumerate() {
            let size_i = self
                .size
                .get(i)
                .copied()
                .ok_or(FunctionInterpolationError::EncodeIndexError)?;
            let max_i_idx = size_i.saturating_sub(1);
            let low = enc
                .floor()
                .to_usize()
                .ok_or(FunctionInterpolationError::SampleIndexOutOfBounds)?;
            let high = low.saturating_add(1).min(max_i_idx);
            idx_low.push(low);
            idx_high.push(high);
            fracs.push(enc.fract());
        }

        // Compute row-major strides: stride[i] = product(size[i+1..m]).
        let mut strides = vec![1usize; m];
        for i in (0..m.saturating_sub(1)).rev() {
            let next = i.saturating_add(1);
            let next_size = self
                .size
                .get(next)
                .copied()
                .ok_or(FunctionInterpolationError::SampleIndexOutOfBounds)?;
            let next_stride = strides
                .get(next)
                .copied()
                .ok_or(FunctionInterpolationError::SampleIndexOutOfBounds)?;
            let s = strides
                .get_mut(i)
                .ok_or(FunctionInterpolationError::SampleIndexOutOfBounds)?;
            *s = next_size
                .checked_mul(next_stride)
                .ok_or(FunctionInterpolationError::SampleIndexOutOfBounds)?;
        }

        // Multilinear interpolation: iterate over all 2^m corners of the hypercube.
        let m_u32 =
            u32::try_from(m).map_err(|_| FunctionInterpolationError::SampleIndexOutOfBounds)?;
        let num_corners = 1usize
            .checked_shl(m_u32)
            .ok_or(FunctionInterpolationError::SampleIndexOutOfBounds)?;

        let mut accum = vec![0.0f32; self.output_count];
        for corner in 0..num_corners {
            let mut weight = 1.0f32;
            let mut flat_index = 0usize;

            for dim in 0..m {
                let dim_u32 = u32::try_from(dim)
                    .map_err(|_| FunctionInterpolationError::SampleIndexOutOfBounds)?;
                let use_high = (corner >> dim_u32) & 1 == 1;
                let (idx, frac_part) = if use_high {
                    let i = idx_high
                        .get(dim)
                        .copied()
                        .ok_or(FunctionInterpolationError::SampleIndexOutOfBounds)?;
                    let f = fracs
                        .get(dim)
                        .copied()
                        .ok_or(FunctionInterpolationError::SampleIndexOutOfBounds)?;
                    (i, f)
                } else {
                    let i = idx_low
                        .get(dim)
                        .copied()
                        .ok_or(FunctionInterpolationError::SampleIndexOutOfBounds)?;
                    let f = fracs
                        .get(dim)
                        .copied()
                        .ok_or(FunctionInterpolationError::SampleIndexOutOfBounds)?;
                    (i, 1.0 - f)
                };
                weight *= frac_part;

                let stride = strides
                    .get(dim)
                    .copied()
                    .ok_or(FunctionInterpolationError::SampleIndexOutOfBounds)?;
                flat_index = flat_index
                    .checked_add(
                        idx.checked_mul(stride)
                            .ok_or(FunctionInterpolationError::SampleIndexOutOfBounds)?,
                    )
                    .ok_or(FunctionInterpolationError::SampleIndexOutOfBounds)?;
            }

            for (j, accum_j) in accum.iter_mut().enumerate() {
                let sample_idx = flat_index
                    .checked_mul(self.output_count)
                    .and_then(|v| v.checked_add(j))
                    .ok_or(FunctionInterpolationError::SampleIndexOutOfBounds)?;
                let sample = self
                    .samples
                    .get(sample_idx)
                    .copied()
                    .ok_or(FunctionInterpolationError::SampleIndexOutOfBounds)?;
                *accum_j += weight * sample;
            }
        }

        // Apply decode mapping and range clamping (ISO 32000 §7.10.2).
        let mut outputs = Vec::with_capacity(self.output_count);
        for (j, &interp) in accum.iter().enumerate() {
            let (decode_min, decode_max) =
                get_pair(&self.decode, j).ok_or(FunctionInterpolationError::EncodeIndexError)?;
            let decoded = decode_min + interp * (decode_max - decode_min);

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
        objects: &dyn ObjectResolver,
    ) -> Result<Function, FunctionReadError> {
        let stream = object.try_stream(objects)?;
        let dictionary = &stream.dictionary;

        // /Domain: Required. Array of 2*m numbers defining input domain.
        let domain = dictionary
            .get_or_err("Domain")?
            .try_vec_of::<f32>(objects)?;

        // /Range: Required for sampled functions. Array of 2*n numbers.
        let range = dictionary.get_or_err("Range")?.try_vec_of::<f32>(objects)?;

        // /Size: Required. Array of m integers specifying samples per input dimension.
        let size = dictionary
            .get_or_err("Size")?
            .try_vec_of::<usize>(objects)?;
        if size.is_empty() {
            return Err(FunctionReadError::InvalidSizeArray);
        }

        let output_count = range.len() / 2;

        // /BitsPerSample: Required. Must be 1, 2, 4, 8, 12, 16, 24, or 32.
        let bits_per_sample = dictionary
            .get_or_err("BitsPerSample")?
            .try_number::<usize>(objects)?;
        if !matches!(bits_per_sample, 1 | 2 | 4 | 8 | 12 | 16 | 24 | 32) {
            return Err(FunctionReadError::InvalidBitsPerSample);
        }

        // /Order: Optional. 1 = linear (default), 3 = cubic spline.
        let order = dictionary
            .get("Order")
            .map(|o| {
                InterpolationOrder::from_i32(o.try_number::<i32>(objects)?)
                    .ok_or(FunctionReadError::InvalidOrder)
            })
            .transpose()?
            .unwrap_or_default();

        // /Encode: Optional. Defaults to [0, Size[i]-1] per dimension.
        let encode = dictionary
            .get("Encode")
            .map(|o| o.try_vec_of::<f32>(objects))
            .transpose()?
            .unwrap_or_else(|| {
                size.iter()
                    .flat_map(|&s| {
                        // size[i] values fit in usize; to_f32() always returns Some here
                        let max_f32 = s.saturating_sub(1).to_f32().unwrap_or(0.0);
                        [0.0, max_f32]
                    })
                    .collect()
            });

        // /Decode: Optional. Defaults to Range values.
        let decode = dictionary
            .get("Decode")
            .map(|o| o.try_vec_of::<f32>(objects))
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal SampledFunction directly for unit testing.
    fn make_sampled(size: Vec<usize>, samples: Vec<f32>, output_count: usize) -> SampledFunction {
        let m = size.len();
        let n = output_count;
        SampledFunction {
            size,
            bits_per_sample: 8,
            order: InterpolationOrder::Linear,
            encode: (0..m).flat_map(|_| [0.0_f32, 1.0]).collect(),
            decode: (0..n).flat_map(|_| [0.0_f32, 1.0]).collect(),
            domain: (0..m).flat_map(|_| [0.0_f32, 1.0]).collect(),
            range: (0..n).flat_map(|_| [0.0_f32, 1.0]).collect(),
            samples,
            output_count,
        }
    }

    #[test]
    fn test_1d_midpoint() {
        // size=[2], samples=[0.0, 1.0]: midpoint should give 0.5
        let f = make_sampled(vec![2], vec![0.0, 1.0], 1);
        let out = f.interpolate(&[0.5]).unwrap();
        assert!((out[0] - 0.5).abs() < 1e-6, "expected 0.5, got {}", out[0]);
    }

    #[test]
    fn test_1d_endpoints() {
        let f = make_sampled(vec![2], vec![0.25, 0.75], 1);
        let lo = f.interpolate(&[0.0]).unwrap();
        let hi = f.interpolate(&[1.0]).unwrap();
        assert!((lo[0] - 0.25).abs() < 1e-6);
        assert!((hi[0] - 0.75).abs() < 1e-6);
    }

    #[test]
    fn test_2d_bilinear() {
        // size=[2,2], 2 samples: corners at (0,0)=0, (1,0)=1, (0,1)=0, (1,1)=1
        // layout: sample[i0 * size[1] + i1]
        // (0,0)=0.0, (0,1)=0.0, (1,0)=1.0, (1,1)=1.0
        let samples = vec![0.0, 0.0, 1.0, 1.0];
        let f = make_sampled(vec![2, 2], samples, 1);
        // At (x0=0.5, x1=anything): should interpolate between rows → 0.5
        let out = f.interpolate(&[0.5, 0.0]).unwrap();
        assert!((out[0] - 0.5).abs() < 1e-6, "expected 0.5, got {}", out[0]);
    }

    #[test]
    fn test_insufficient_inputs() {
        let f = make_sampled(vec![2, 2], vec![0.0, 0.0, 1.0, 1.0], 1);
        assert!(matches!(
            f.interpolate(&[0.5]),
            Err(FunctionInterpolationError::InsufficientInputs { .. })
        ));
    }

    #[test]
    fn test_cubic_returns_error() {
        let mut f = make_sampled(vec![4], vec![0.0, 0.25, 0.75, 1.0], 1);
        f.order = InterpolationOrder::Cubic;
        assert!(matches!(
            f.interpolate(&[0.5]),
            Err(FunctionInterpolationError::CubicInterpolationNotSupported)
        ));
    }
}
