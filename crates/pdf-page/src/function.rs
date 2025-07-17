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
pub enum FunctionData {
    Exponential {
        c0: Vec<f32>,
        c1: Vec<f32>,
        exponent: f32,
        domain: [f32; 2],
    },
    Stitching {
        functions: Vec<Function>, // Or Vec<Ref>, depending on your object model
        bounds: Vec<f32>,
        encode: Vec<f32>,
        domain: [f32; 2],
    },
}

#[derive(Debug)]
pub struct Function {
    pub function_type: FunctionType,
    pub data: FunctionData,
}

impl Function {
    pub fn domain(&self) -> [f32; 2] {
        match &self.data {
            FunctionData::Exponential { domain, .. } => *domain,
            FunctionData::Stitching { domain, .. } => *domain,
        }
    }

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

        if let FunctionData::Exponential {
            c0,
            c1,
            exponent,
            domain,
        } = &self.data
        {
            if c0.len() != c1.len() {
                return Err(FunctionInterpolationError::MismatchedC0C1Length);
            }

            if domain[0] >= domain[1] {
                return Err(FunctionInterpolationError::InvalidDomain);
            }

            // 1. Clip the input value `x` to the function's domain.
            let x_clipped = x.max(domain[0]).min(domain[1]);

            // 2. Normalize the clipped value to the interval [0, 1].
            let x_normalized = (x_clipped - domain[0]) / (domain[1] - domain[0]);

            // 3. Apply the interpolation formula for each component.
            let result = c0
                .iter()
                .zip(c1.iter())
                .map(|(&c0_i, &c1_i)| c0_i + x_normalized.powf(*exponent) * (c1_i - c0_i))
                .collect();

            Ok(result)
        } else {
            Err(FunctionInterpolationError::UnsupportedFunctionType(
                self.function_type,
            ))
        }
    }
}

impl FromDictionary for Function {
    const KEY: &'static str = "Function";
    type ResultType = Self;
    type ErrorType = FunctionReadError;

    fn from_dictionary(
        dictionary: &Dictionary,
        objects: &ObjectCollection,
    ) -> Result<Self::ResultType, Self::ErrorType> {
        println!("Read function dictionary: {dictionary:?}");

        let function_type_val = dictionary
            .get("FunctionType")
            .ok_or(FunctionReadError::MissingFunctionType)?;

        let function_type_int = function_type_val
            .as_number::<i32>()
            .map_err(|_| FunctionReadError::InvalidFunctionType)?;

        let function_type = FunctionType::from_i32(function_type_int)
            .ok_or(FunctionReadError::InvalidFunctionType)?;

        match function_type {
            FunctionType::ExponentialInterpolation => {
                let mut c0 = vec![];
                for obj in dictionary.get_array("C0").unwrap().iter() {
                    let value = obj.as_number::<f32>().map_err(|e| {
                        FunctionReadError::NumericConversionError {
                            entry_description: "C0",
                            source: e,
                        }
                    })?;
                    c0.push(value);
                }

                let mut c1 = vec![];
                for obj in dictionary.get_array("C1").unwrap().iter() {
                    let value = obj.as_number::<f32>().map_err(|e| {
                        FunctionReadError::NumericConversionError {
                            entry_description: "C1",
                            source: e,
                        }
                    })?;
                    c1.push(value);
                }

                let exponent = dictionary
                    .get("N")
                    .ok_or(FunctionReadError::InvalidFunctionType)?
                    .as_number::<f32>()
                    .map_err(|e| FunctionReadError::NumericConversionError {
                        entry_description: "N",
                        source: e,
                    })?;

                let mut domain = [0.0_f32, 1.0_f32];
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
                    return Err(FunctionReadError::InvalidFunctionType);
                }

                Ok(Function {
                    function_type,
                    data: FunctionData::Exponential {
                        c0,
                        c1,
                        exponent,
                        domain,
                    },
                })
            }
            FunctionType::Stitching => {
                // Parse Functions array
                let functions_arr = dictionary
                    .get_array("Functions")
                    .ok_or(FunctionReadError::InvalidFunctionType)?;
                let mut functions = Vec::new();
                for obj in functions_arr.iter() {
                    let dict = obj
                        .as_dictionary()
                        .ok_or(FunctionReadError::InvalidFunctionType)?;
                    let func = Function::from_dictionary(dict, objects)?;
                    functions.push(func);
                }

                // Parse Bounds array
                let bounds_arr = dictionary
                    .get_array("Bounds")
                    .ok_or(FunctionReadError::InvalidFunctionType)?;
                let mut bounds = Vec::new();
                for obj in bounds_arr.iter() {
                    let value = obj.as_number::<f32>().map_err(|e| {
                        FunctionReadError::NumericConversionError {
                            entry_description: "Bounds",
                            source: e,
                        }
                    })?;
                    bounds.push(value);
                }

                // Parse Encode array
                let encode_arr = dictionary
                    .get_array("Encode")
                    .ok_or(FunctionReadError::InvalidFunctionType)?;
                let mut encode = Vec::new();
                for obj in encode_arr.iter() {
                    let value = obj.as_number::<f32>().map_err(|e| {
                        FunctionReadError::NumericConversionError {
                            entry_description: "Encode",
                            source: e,
                        }
                    })?;
                    encode.push(value);
                }

                // Parse Domain array
                let mut domain = [0.0_f32, 1.0_f32];
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
                    return Err(FunctionReadError::InvalidFunctionType);
                }

                Ok(Function {
                    function_type,
                    data: FunctionData::Stitching {
                        functions,
                        bounds,
                        encode,
                        domain,
                    },
                })
            }
            _ => Err(FunctionReadError::InvalidFunctionType),
        }
    }
}
