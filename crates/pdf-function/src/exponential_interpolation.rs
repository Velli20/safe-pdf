use pdf_object_reader::{
    object_lookup::ObjectLookupExt, object_resolver::ObjectResolver, object_variant::ObjectVariant,
};

use crate::{
    error::FunctionReadError,
    function::{Function, FunctionImpl, clamp_and_normalize, get_pair},
    function_interpolation_error::FunctionInterpolationError,
};

#[derive(Debug, Clone)]
pub struct ExponentialFunction {
    /// Output values at `domain[0]`.
    c0: Vec<f32>,
    /// Output values at `domain[1]`.
    c1: Vec<f32>,
    /// Interpolation exponent.
    exponent: f32,
    /// Input domain `[min, max]`.
    domain: [f32; 2],
    /// Optional output range `[min0, max0, min1, max1, ...]` for clamping (ISO 32000 §7.10.3).
    range: Option<Vec<f32>>,
}

impl ExponentialFunction {
    pub fn new(c0: Vec<f32>, c1: Vec<f32>, exponent: f32, domain: [f32; 2]) -> Self {
        Self {
            c0,
            c1,
            exponent,
            domain,
            range: None,
        }
    }
}

impl FunctionImpl for ExponentialFunction {
    /// Interpolates using exponential interpolation (Type 2 function).
    ///
    /// Formula: `result[i] = c0[i] + x^N * (c1[i] - c0[i])`
    /// Output is clamped to `/Range` if present (ISO 32000 §7.10.3).
    fn interpolate(&self, inputs: &[f32]) -> Result<Vec<f32>, FunctionInterpolationError> {
        let x = inputs
            .first()
            .copied()
            .ok_or(FunctionInterpolationError::InsufficientInputs {
                expected: 1,
                got: 0,
            })?;

        // Normalize input to [0, 1]
        let x_normalized = clamp_and_normalize(x, self.domain)
            .ok_or(FunctionInterpolationError::InvalidDomainInterval)?;

        // Guard against 0^(negative) which is undefined
        if self.exponent < 0.0 && x_normalized == 0.0 {
            return Err(FunctionInterpolationError::UndefinedExponentiationAtZero);
        }

        // Apply interpolation formula: c0 + x^N * (c1 - c0)
        let pow = x_normalized.powf(self.exponent);
        let mut result: Vec<f32> = self
            .c0
            .iter()
            .zip(self.c1.iter())
            .map(|(&c0_i, &c1_i)| c0_i + pow * (c1_i - c0_i))
            .collect();

        // Clamp outputs to Range if defined (ISO 32000 §7.10.3).
        if let Some(range) = &self.range {
            for (i, val) in result.iter_mut().enumerate() {
                if let Some((min, max)) = get_pair(range, i) {
                    *val = val.clamp(min, max);
                }
            }
        }

        Ok(result)
    }

    fn domain(&self) -> Option<[f32; 2]> {
        Some(self.domain)
    }

    /// Parses a Type 2 (Exponential Interpolation) function.
    fn parse(
        object: &ObjectVariant,
        objects: &dyn ObjectResolver,
    ) -> Result<Function, FunctionReadError> {
        let dictionary = object.try_dictionary(objects)?;
        let domain = dictionary.required_array_of::<f32, 2>(b"Domain", objects)?;

        // /C0: Output values at domain[0]. Defaults to [0.0].
        let c0 = dictionary
            .optional_vec_of::<f32>(b"C0", objects)?
            .unwrap_or_else(|| vec![0.0]);

        // /C1: Output values at domain[1]. Defaults to [1.0].
        let c1 = dictionary
            .optional_vec_of::<f32>(b"C1", objects)?
            .unwrap_or_else(|| vec![1.0]);

        // Validate C0/C1 length match at parse time.
        if c0.len() != c1.len() {
            return Err(FunctionReadError::MismatchedC0C1Length);
        }

        // /N: Interpolation exponent (required).
        let exponent = dictionary.required_number::<f32>(b"N", objects)?;

        // /Range: Optional. Output range for clamping (ISO 32000 §7.10.3).
        let range = dictionary.optional_vec_of::<f32>(b"Range", objects)?;

        Ok(Function::Exponential(ExponentialFunction {
            c0,
            c1,
            exponent,
            domain,
            range,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_exp(c0: Vec<f32>, c1: Vec<f32>, n: f32) -> ExponentialFunction {
        ExponentialFunction {
            c0,
            c1,
            exponent: n,
            domain: [0.0, 1.0],
            range: None,
        }
    }

    #[test]
    fn test_linear_n1() {
        // N=1: result = c0 + x*(c1-c0)
        let f = make_exp(vec![0.0], vec![1.0], 1.0);
        let out = f.interpolate(&[0.5]).unwrap();
        assert!((out[0] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn test_range_clamping() {
        let mut f = make_exp(vec![0.0], vec![2.0], 1.0);
        f.range = Some(vec![0.0, 1.0]);
        // Without clamping: x=1.0 → c0 + 1*(c1-c0) = 2.0; clamped to 1.0
        let out = f.interpolate(&[1.0]).unwrap();
        assert!((out[0] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_no_inputs_errors() {
        let f = make_exp(vec![0.0], vec![1.0], 1.0);
        assert!(matches!(
            f.interpolate(&[]),
            Err(FunctionInterpolationError::InsufficientInputs { .. })
        ));
    }

    #[test]
    fn test_negative_exponent_at_zero_errors() {
        let f = make_exp(vec![0.0], vec![1.0], -1.0);
        assert!(matches!(
            f.interpolate(&[0.0]),
            Err(FunctionInterpolationError::UndefinedExponentiationAtZero)
        ));
    }
}
