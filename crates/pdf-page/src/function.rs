use std::panic;

use pdf_object::{
    dictionary::Dictionary, error::ObjectError, object_collection::ObjectCollection,
    traits::FromDictionary,
};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum FunctionReadError {
    #[error("Missing /FunctionType key")]
    MissingFunctionType,
    #[error("Invalid /FunctionType value")]
    InvalidFunctionType,
    #[error("Failed to convert PDF value to number for '{entry_description}': {source}")]
    NumericConversionError {
        entry_description: &'static str,
        #[source]
        source: ObjectError,
    },
}

#[derive(Debug, Error)]
pub enum FunctionInterpolationError {
    #[error("Interpolation is not supported for function type {0:?}")]
    UnsupportedFunctionType(FunctionType),
    #[error("C0 and C1 arrays must have the same length")]
    MismatchedC0C1Length,
    #[error("Domain must be an increasing interval (domain[0] < domain[1])")]
    InvalidDomain,
}

/// Represents the type of a PDF Function object, as defined in PDF 1.7, Table 38.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionType {
    /// Type 0, a sampled function that uses a table of sample values to define the function.
    Sampled = 0,
    /// Type 2, an exponential interpolation function.
    ExponentialInterpolation = 2,
    /// Type 3, a stitching function that combines several other functions into a single function.
    Stitching = 3,
    /// Type 4, a PostScript calculator function that uses a small subset of the PostScript language
    /// to describe the function.
    PostScriptCalculator = 4,
}

impl FunctionType {
    /// Creates a `FunctionType` from an integer value.
    pub fn from_i32(val: i32) -> Option<Self> {
        match val {
            0 => Some(FunctionType::Sampled),
            2 => Some(FunctionType::ExponentialInterpolation),
            3 => Some(FunctionType::Stitching),
            4 => Some(FunctionType::PostScriptCalculator),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub struct Function {
    pub c0: Vec<f32>,
    pub c1: Vec<f32>,
    pub function_type: FunctionType,
    pub exponent: f32,
    pub domain: [f32; 2],
}

impl Function {
    /// Interpolates an input value `x` according to the function's definition.
    ///
    /// As per PDF 1.7 spec, section 7.10.3, for a given input `x`, the output `y_j` for
    /// each component `j` is calculated as:
    /// `y_j = C0_j + normalized_x^N * (C1_j - C0_j)`
    ///
    /// where `normalized_x` is the input `x` clipped to the function's `domain` and then
    /// mapped to the interval [0, 1].
    ///
    /// # Parameters
    ///
    /// - `x`: The input value to the function.
    ///
    /// # Returns
    ///
    /// A `Result` containing a `Vec<f32>` of the output values on success, or a
    /// `FunctionInterpolationError` on failure.
    pub fn interpolate(&self, x: f32) -> Result<Vec<f32>, FunctionInterpolationError> {
        if self.function_type != FunctionType::ExponentialInterpolation {
            return Err(FunctionInterpolationError::UnsupportedFunctionType(
                self.function_type,
            ));
        }

        if self.c0.len() != self.c1.len() {
            return Err(FunctionInterpolationError::MismatchedC0C1Length);
        }

        if self.domain[0] >= self.domain[1] {
            return Err(FunctionInterpolationError::InvalidDomain);
        }

        // 1. Clip the input value `x` to the function's domain.
        let x_clipped = x.max(self.domain[0]).min(self.domain[1]);

        // 2. Normalize the clipped value to the interval [0, 1].
        let x_normalized = (x_clipped - self.domain[0]) / (self.domain[1] - self.domain[0]);

        // 3. Apply the interpolation formula for each component.
        let result = self
            .c0
            .iter()
            .zip(self.c1.iter())
            .map(|(&c0_i, &c1_i)| c0_i + x_normalized.powf(self.exponent) * (c1_i - c0_i))
            .collect();

        Ok(result)
    }
}

impl FromDictionary for Function {
    const KEY: &'static str = "Function";

    type ResultType = Self;

    type ErrorType = FunctionReadError;

    fn from_dictionary(
        dictionary: &Dictionary,
        _objects: &ObjectCollection,
    ) -> Result<Self::ResultType, Self::ErrorType> {
        println!("Read function dictionary: {dictionary:?}");

        let mut c0 = vec![];
        for obj in dictionary.get_array("C0").unwrap().iter() {
            let value =
                obj.as_number::<f32>()
                    .map_err(|e| FunctionReadError::NumericConversionError {
                        entry_description: "width in [w1...wn] array",
                        source: e,
                    })?;
            c0.push(value);
        }

        let mut c1 = vec![];
        for obj in dictionary.get_array("C1").unwrap().iter() {
            let value =
                obj.as_number::<f32>()
                    .map_err(|e| FunctionReadError::NumericConversionError {
                        entry_description: "width in [w1...wn] array",
                        source: e,
                    })?;
            c1.push(value);
        }

        let function_type_val = dictionary
            .get("FunctionType")
            .ok_or(FunctionReadError::MissingFunctionType)?;

        let function_type_int = function_type_val
            .as_number::<i32>()
            .map_err(|_| FunctionReadError::InvalidFunctionType)?;

        let function_type = FunctionType::from_i32(function_type_int)
            .ok_or(FunctionReadError::InvalidFunctionType)?;

        let exponent = if let Some(obj) = dictionary.get("N") {
            obj.as_number::<f32>()
                .map_err(|e| FunctionReadError::NumericConversionError {
                    entry_description: "width in [w1...wn] array",
                    source: e,
                })?
        } else {
            panic!("No exponent found");
        };

        let mut domain = [0.0_f32, 0.1_f32];

        if let Some(obj) = dictionary.get("Domain") {
            let arr = obj
                .as_array()
                .ok_or(FunctionReadError::InvalidFunctionType)?;
            if arr.len() != 2 {
                return Err(FunctionReadError::InvalidFunctionType);
            }
            for (i, obj) in arr.iter().enumerate() {
                domain[i] = obj.as_number::<f32>().map_err(|e| {
                    FunctionReadError::NumericConversionError {
                        entry_description: "Domain",
                        source: e,
                    }
                })?;
            }
        } else {
            panic!("No domain found");
        }
        Ok(Function {
            c0,
            c1,
            function_type,
            exponent,
            domain,
        })
    }
}
