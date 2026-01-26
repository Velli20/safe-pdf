use pdf_object::{ObjectVariant, object_collection::ObjectCollection};

use crate::functions::{
    Function, FunctionImpl, FunctionInterpolationError, FunctionReadError, clamp_and_normalize,
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
}

impl ExponentialFunction {
    #[allow(dead_code)]
    pub(crate) fn new(c0: Vec<f32>, c1: Vec<f32>, exponent: f32, domain: [f32; 2]) -> Self {
        Self {
            c0,
            c1,
            exponent,
            domain,
        }
    }
}

impl FunctionImpl for ExponentialFunction {
    /// Interpolates using exponential interpolation (Type 2 function).
    ///
    /// Formula: `result[i] = c0[i] + x^N * (c1[i] - c0[i])`
    fn interpolate(&self, x: f32) -> Result<Vec<f32>, FunctionInterpolationError> {
        if self.c0.len() != self.c1.len() {
            return Err(FunctionInterpolationError::MismatchedC0C1Length);
        }

        // Normalize input to [0, 1]
        let x_normalized =
            clamp_and_normalize(x, self.domain).ok_or(FunctionInterpolationError::InvalidDomain)?;

        // Guard against 0^(negative) which is undefined
        if self.exponent < 0.0 && x_normalized == 0.0 {
            return Err(FunctionInterpolationError::NegativeExponentAtZero);
        }

        // Apply interpolation formula: c0 + x^N * (c1 - c0)
        let pow = x_normalized.powf(self.exponent);
        let result = self
            .c0
            .iter()
            .zip(self.c1.iter())
            .map(|(&c0_i, &c1_i)| c0_i + pow * (c1_i - c0_i))
            .collect();

        Ok(result)
    }

    fn domain(&self) -> Option<[f32; 2]> {
        Some(self.domain)
    }

    /// Parses a Type 2 (Exponential Interpolation) function.
    fn parse(
        object: &ObjectVariant,
        objects: &ObjectCollection,
    ) -> Result<Function, FunctionReadError> {
        let dictionary = object.try_dictionary(objects)?;
        let domain = dictionary.get_or_err("Domain")?.as_array_of::<f32, 2>()?;

        // /C0: Output values at domain[0]. Defaults to [0.0].
        let c0 = dictionary
            .get("C0")
            .map(ObjectVariant::as_vec_of::<f32>)
            .transpose()?
            .unwrap_or_else(|| vec![0.0]);

        // /C1: Output values at domain[1]. Defaults to [1.0].
        let c1 = dictionary
            .get("C1")
            .map(ObjectVariant::as_vec_of::<f32>)
            .transpose()?
            .unwrap_or_else(|| vec![1.0]);

        // /N: Interpolation exponent (required).
        let exponent = dictionary.get_or_err("N")?.as_number::<f32>()?;

        Ok(Function::Exponential(ExponentialFunction {
            c0,
            c1,
            exponent,
            domain,
        }))
    }
}
