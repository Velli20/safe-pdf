use std::cmp::Ordering;

use pdf_graphics::interval::Interval;
use pdf_object_reader::{
    object_lookup::ObjectLookupExt, object_resolver::ObjectResolver, object_variant::ObjectVariant,
};

use crate::{
    error::FunctionReadError,
    function::{Function, FunctionImpl},
    function_interpolation_error::FunctionInterpolationError,
};

#[derive(Debug, Clone)]
pub struct StitchingFunction {
    /// Sub-functions to be stitched together, each with the interval its
    /// sub-domain is mapped onto.
    segments: Vec<(Function, Interval)>,
    /// Boundary values dividing the domain into sub-domains.
    bounds: Vec<f32>,
    /// Input domain.
    domain: Interval,
}

impl StitchingFunction {
    /// Returns the sub-domain `[b0, b1]` for the given segment index.
    fn subdomain(&self, index: usize) -> Interval {
        let b0 = index
            .checked_sub(1)
            .and_then(|prev| self.bounds.get(prev))
            .copied()
            .unwrap_or(self.domain.min);
        let b1 = self.bounds.get(index).copied().unwrap_or(self.domain.max);
        Interval::new(b0, b1)
    }
}

impl FunctionImpl for StitchingFunction {
    /// Interpolates using stitching (Type 3 function).
    ///
    /// Selects the appropriate sub-function based on bounds and maps the input
    /// to the sub-function's domain using the encode array.
    fn interpolate(&self, inputs: &[f32]) -> Result<Vec<f32>, FunctionInterpolationError> {
        let x = inputs
            .first()
            .copied()
            .ok_or(FunctionInterpolationError::InsufficientInputs {
                expected: 1,
                got: 0,
            })?;

        // Reject NaN bounds: partial_cmp returns None for NaN, which would silently
        // return the wrong segment index.
        if self.bounds.iter().any(|b| b.is_nan()) {
            return Err(FunctionInterpolationError::BoundsContainNaN);
        }

        // Clamp input to domain
        let x_clamped = self.domain.clamp(x);

        // Find which sub-function to use via binary search on bounds.
        // SAFETY: NaN-free bounds checked above; partial_cmp always returns Some.
        let index = match self
            .bounds
            .binary_search_by(|b| b.partial_cmp(&x_clamped).unwrap_or(Ordering::Less))
        {
            Ok(pos) => pos.saturating_add(1), // Exact match: use next segment
            Err(pos) => pos,
        };

        // Map the input from this segment's sub-domain onto its encode interval
        let (func, encode) = self
            .segments
            .get(index)
            .ok_or(FunctionInterpolationError::FunctionDataIndexOutOfBounds)?;
        let x_mapped = self.subdomain(index).remap(x_clamped, encode);

        // Evaluate the selected sub-function
        func.interpolate(&[x_mapped])
    }

    fn domain(&self) -> Option<Interval> {
        Some(self.domain)
    }

    fn parse(
        object: &ObjectVariant,
        objects: &dyn ObjectResolver,
    ) -> Result<Function, FunctionReadError> {
        let dictionary = object.try_dictionary(objects)?;

        let domain = dictionary.required_interval(b"Domain", objects)?;

        // Parse /Functions array (sub-functions to stitch together)
        let functions_arr = dictionary.required_array(b"Functions", objects)?;
        let functions = functions_arr
            .iter()
            .map(|obj| Function::parse(obj, objects))
            .collect::<Result<Vec<_>, _>>()?;

        // Parse /Bounds array (boundaries between sub-functions)
        let mut bounds = dictionary.required_vec_of::<f32>(b"Bounds", objects)?;

        // Parse /Encode array (input mapping for each sub-function)
        let encode = dictionary.required_intervals(b"Encode", objects)?;

        // Validate structural relationships. Producers sometimes pad /Bounds or /Encode, so
        // extra trailing values are dropped rather than rejected.
        let expected_bounds = functions
            .len()
            .checked_sub(1)
            .ok_or(FunctionReadError::InvalidBoundsLength)?;
        if bounds.len() < expected_bounds {
            return Err(FunctionReadError::InvalidBoundsLength);
        }
        bounds.truncate(expected_bounds);

        if encode.len() < functions.len() {
            return Err(FunctionReadError::InvalidEncodeLength);
        }
        let segments = functions.into_iter().zip(encode).collect();

        Ok(Function::Stitching(StitchingFunction {
            segments,
            bounds,
            domain,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        exponential_interpolation::ExponentialFunction,
        function::{Function, FunctionImpl},
    };

    fn make_linear_exp(c0: f32, c1: f32, domain: [f32; 2]) -> Function {
        Function::Exponential(ExponentialFunction::new(
            vec![c0],
            vec![c1],
            1.0,
            Interval::from(domain),
        ))
    }

    fn make_stitch(
        bounds: Vec<f32>,
        encode: Vec<f32>,
        functions: Vec<Function>,
    ) -> StitchingFunction {
        StitchingFunction {
            segments: functions
                .into_iter()
                .zip(Interval::from_pairs(&encode))
                .collect(),
            bounds,
            domain: Interval::UNIT,
        }
    }

    #[test]
    fn test_two_segments() {
        // Domain [0,1] split at 0.5: left segment maps [0,0.5]→[0,1], right [0.5,1]→[0,1]
        let f = make_stitch(
            vec![0.5],
            vec![0.0, 1.0, 0.0, 1.0],
            vec![
                make_linear_exp(0.0, 1.0, [0.0, 1.0]),
                make_linear_exp(0.5, 1.0, [0.0, 1.0]),
            ],
        );
        // x=0.25 is in left segment, mapped to e=0.5 → f(0.5) = 0.5
        let out = f.interpolate(&[0.25]).unwrap();
        assert!((out[0] - 0.5).abs() < 1e-5, "got {}", out[0]);
    }

    #[test]
    fn test_no_inputs_errors() {
        let f = make_stitch(
            vec![0.5],
            vec![0.0, 1.0, 0.0, 1.0],
            vec![
                make_linear_exp(0.0, 1.0, [0.0, 1.0]),
                make_linear_exp(0.0, 1.0, [0.0, 1.0]),
            ],
        );
        assert!(matches!(
            f.interpolate(&[]),
            Err(FunctionInterpolationError::InsufficientInputs { .. })
        ));
    }

    #[test]
    fn test_nan_bounds_error() {
        let f = make_stitch(
            vec![f32::NAN],
            vec![0.0, 1.0, 0.0, 1.0],
            vec![
                make_linear_exp(0.0, 1.0, [0.0, 1.0]),
                make_linear_exp(0.0, 1.0, [0.0, 1.0]),
            ],
        );
        assert!(matches!(
            f.interpolate(&[0.5]),
            Err(FunctionInterpolationError::BoundsContainNaN)
        ));
    }
}
