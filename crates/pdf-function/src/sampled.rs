use num_derive::FromPrimitive;
use num_traits::{FromPrimitive, ToPrimitive};
use pdf_decode::decode_normalized_samples;
use pdf_graphics::interval::Interval;
use pdf_object_reader::{
    object_lookup::ObjectLookupExt, object_resolver::ObjectResolver, object_variant::ObjectVariant,
};

use crate::{
    error::FunctionReadError,
    function::{Function, FunctionImpl},
    function_interpolation_error::FunctionInterpolationError,
};

#[derive(Debug, Clone, Copy, PartialEq, FromPrimitive, Default)]
enum InterpolationOrder {
    #[default]
    Linear = 1,
    Cubic = 3,
}

/// One input dimension of a sample table.
#[derive(Debug, Clone)]
struct SampleAxis {
    /// Input domain of this dimension.
    domain: Interval,
    /// Interval the domain is mapped onto, in sample index units.
    encode: Interval,
    /// Largest sample index along this axis (`Size[i] - 1`).
    last: usize,
    /// `last` as f32, used to clamp encoded coordinates.
    max_index: f32,
    /// Distance in the sample table between neighbouring indices along this axis,
    /// already multiplied by the number of outputs.
    stride: usize,
}

/// The two sample offsets bracketing an input along one axis, and the weight of the upper one.
struct AxisPosition {
    low: usize,
    high: usize,
    frac: f32,
}

impl SampleAxis {
    /// Encodes `x` into this axis's sample coordinates and returns the bracketing offsets.
    fn position(&self, x: f32) -> Result<AxisPosition, FunctionInterpolationError> {
        let encoded = self
            .domain
            .remap(self.domain.clamp(x), &self.encode)
            .clamp(0.0, self.max_index);
        let low = encoded
            .floor()
            .to_usize()
            .ok_or(FunctionInterpolationError::SampleCoordinateOutOfBounds)?;
        let high = low.saturating_add(1).min(self.last);
        // `parse` proved `last * stride` fits in the sample table, so neither product saturates.
        Ok(AxisPosition {
            low: low.saturating_mul(self.stride),
            high: high.saturating_mul(self.stride),
            frac: encoded.fract(),
        })
    }
}

/// One output of a sample table.
#[derive(Debug, Clone)]
struct SampleOutput {
    /// Maps sample values onto the output.
    decode: Interval,
    /// Interval the output is clamped to.
    range: Interval,
}

#[derive(Debug, Clone)]
pub struct SampledFunction {
    /// Interpolation order.
    order: InterpolationOrder,
    /// Input dimensions, outermost first.
    axes: Vec<SampleAxis>,
    /// Outputs produced at each sample point.
    outputs: Vec<SampleOutput>,
    /// The decoded sample values, row-major with the outputs of one sample point adjacent.
    samples: Vec<f32>,
}

impl FunctionImpl for SampledFunction {
    /// Interpolates using a sampled function (Type 0).
    ///
    /// Supports N-dimensional multilinear interpolation as defined in ISO 32000 §7.10.2.
    /// Cubic spline interpolation (Order=3) is recognised in the dictionary but not yet
    /// implemented; it returns [`FunctionInterpolationError::CubicInterpolationUnsupported`].
    fn interpolate(&self, inputs: &[f32]) -> Result<Vec<f32>, FunctionInterpolationError> {
        let inputs = inputs.get(..self.axes.len()).ok_or(
            FunctionInterpolationError::InsufficientInputs {
                expected: self.axes.len(),
                got: inputs.len(),
            },
        )?;

        if self.order == InterpolationOrder::Cubic {
            return Err(FunctionInterpolationError::CubicInterpolationUnsupported);
        }

        let positions = self
            .axes
            .iter()
            .zip(inputs)
            .map(|(axis, &x)| axis.position(x))
            .collect::<Result<Vec<_>, _>>()?;

        // Expand the 2^m corners of the enclosing hypercube one axis at a time,
        // as (sample offset, weight) pairs.
        let corners = positions.iter().fold(vec![(0usize, 1.0f32)], |corners, p| {
            corners
                .into_iter()
                .flat_map(|(offset, weight)| {
                    [
                        (offset.saturating_add(p.low), weight * (1.0 - p.frac)),
                        (offset.saturating_add(p.high), weight * p.frac),
                    ]
                })
                .collect()
        });

        let output_count = self.outputs.len();
        let mut accum = vec![0.0f32; output_count];
        for (offset, weight) in corners {
            let values = offset
                .checked_add(output_count)
                .and_then(|end| self.samples.get(offset..end))
                .ok_or(FunctionInterpolationError::SampleCoordinateOutOfBounds)?;
            for (sum, &value) in accum.iter_mut().zip(values) {
                *sum += weight * value;
            }
        }

        // Apply decode mapping and range clamping (ISO 32000 §7.10.2).
        Ok(self
            .outputs
            .iter()
            .zip(accum)
            .map(|(output, value)| output.range.clamp(output.decode.lerp(value)))
            .collect())
    }

    fn domain(&self) -> Option<Interval> {
        self.axes.first().map(|axis| axis.domain)
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
        let domain = dictionary.required_intervals(b"Domain", objects)?;

        // /Range: Required for sampled functions. Array of 2*n numbers.
        let range = dictionary.required_intervals(b"Range", objects)?;

        // /Size: Required. Array of m integers specifying samples per input dimension.
        let size = dictionary.required_vec_of::<usize>(b"Size", objects)?;
        if size.is_empty() {
            return Err(FunctionReadError::InvalidSizeArray);
        }

        // /BitsPerSample: Required. Must be 1, 2, 4, 8, 12, 16, 24, or 32.
        let bits_per_sample = dictionary.required_number::<usize>(b"BitsPerSample", objects)?;
        if !matches!(bits_per_sample, 1 | 2 | 4 | 8 | 12 | 16 | 24 | 32) {
            return Err(FunctionReadError::InvalidBitsPerSample);
        }

        // /Order: Optional. 1 = linear (default), 3 = cubic spline.
        let order = dictionary
            .get(b"Order")
            .map(|o| {
                InterpolationOrder::from_i32(o.try_number::<i32>(objects)?)
                    .ok_or(FunctionReadError::InvalidOrder)
            })
            .transpose()?
            .unwrap_or_default();

        // /Encode: Optional. Defaults to [0, Size[i]-1] per dimension.
        let encode = dictionary.optional_intervals(b"Encode", objects)?;
        if domain.len() < size.len() || encode.as_ref().is_some_and(|e| e.len() < size.len()) {
            return Err(FunctionReadError::MismatchedSampleDimensions);
        }

        // /Decode: Optional. Defaults to Range values.
        let decode = dictionary
            .optional_intervals(b"Decode", objects)?
            .unwrap_or_else(|| range.clone());
        if decode.len() != range.len() {
            return Err(FunctionReadError::InvalidDecodeLength);
        }

        // Row-major strides, innermost axis last; the final product is the sample count.
        let mut strides = Vec::with_capacity(size.len());
        let samples_count = size.iter().rev().try_fold(range.len(), |stride, &dim| {
            strides.push(stride);
            stride
                .checked_mul(dim)
                .ok_or(FunctionReadError::InvalidSizeArray)
        })?;
        strides.reverse();

        let axes = size
            .iter()
            .zip(strides)
            .zip(domain)
            .enumerate()
            .map(|(i, ((&dim, stride), domain))| {
                let last = dim.saturating_sub(1);
                // usize to f32 conversion always succeeds.
                let max_index = last.to_f32().unwrap_or(0.0);
                let encode = encode
                    .as_ref()
                    .and_then(|e| e.get(i))
                    .copied()
                    .unwrap_or_else(|| Interval::new(0.0, max_index));
                SampleAxis {
                    domain,
                    encode,
                    last,
                    max_index,
                    stride,
                }
            })
            .collect();

        let outputs = decode
            .into_iter()
            .zip(range)
            .map(|(decode, range)| SampleOutput { decode, range })
            .collect();

        let samples = decode_normalized_samples(stream.raw_data(), bits_per_sample, samples_count)?;

        Ok(Function::Sampled(SampledFunction {
            order,
            axes,
            outputs,
            samples,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal SampledFunction directly for unit testing.
    fn make_sampled(size: Vec<usize>, samples: Vec<f32>, output_count: usize) -> SampledFunction {
        let mut stride = output_count;
        let mut axes: Vec<SampleAxis> = size
            .iter()
            .rev()
            .map(|&dim| {
                let axis = SampleAxis {
                    domain: Interval::UNIT,
                    encode: Interval::UNIT,
                    last: dim - 1,
                    max_index: (dim - 1) as f32,
                    stride,
                };
                stride *= dim;
                axis
            })
            .collect();
        axes.reverse();
        SampledFunction {
            order: InterpolationOrder::Linear,
            axes,
            outputs: vec![
                SampleOutput {
                    decode: Interval::UNIT,
                    range: Interval::UNIT,
                };
                output_count
            ],
            samples,
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
            Err(FunctionInterpolationError::CubicInterpolationUnsupported)
        ));
    }
}
