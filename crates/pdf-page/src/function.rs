use pdf_object::{
    ObjectVariant, dictionary::Dictionary, error::ObjectError, object_collection::ObjectCollection,
    stream, traits::FromDictionary,
};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum FunctionReadError {
    #[error("Missing /FunctionType entry")]
    MissingFunctionType,
    #[error("Missing /Domain entry")]
    MissingDomain,
    #[error("Invalid /FunctionType value")]
    InvalidFunctionType,
    #[error("Failed to convert PDF value to number for '{entry_description}': {source}")]
    NumericConversionError {
        entry_description: &'static str,
        #[source]
        source: ObjectError,
    },
    #[error("Failed to read function value for '{entry_description}': {source}")]
    EntryReadError {
        entry_description: &'static str,
        #[source]
        source: ObjectError,
    },
    #[error("Domain parsing error: {0}")]
    DomainParsingError(#[from] ObjectError),
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
    pub fn domain(&self) -> &[f32; 2] {
        match &self.data {
            FunctionData::Exponential { domain, .. } => domain,
            FunctionData::Stitching { domain, .. } => domain,
        }
    }

    /// Interpolates an input value `x` according to the function's definition.
    ///
    /// # Returns
    ///
    /// A `Result` containing a `Vec<f32>` of the output values on success, or a
    /// `FunctionInterpolationError` on failure.
    pub fn interpolate(&self, x: f32) -> Result<Vec<f32>, FunctionInterpolationError> {
        if let FunctionData::Exponential {
            c0,
            c1,
            exponent,
            domain,
        } = &self.data
        {
            if self.function_type != FunctionType::ExponentialInterpolation {
                return Err(FunctionInterpolationError::UnsupportedFunctionType(
                    self.function_type,
                ));
            }

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
        } else if let FunctionData::Stitching {
            functions,
            bounds,
            encode,
            domain,
        } = &self.data
        {
            let x_clamped = x.clamp(domain[0], domain[1]);

            // Determine which sub-function applies
            let mut index = 0;
            while index < bounds.len() && x_clamped >= bounds[index] {
                index += 1;
            }

            // Determine mapping range
            let (b0, b1) = if index == 0 {
                (domain[0], bounds[0])
            } else if index == bounds.len() {
                (bounds[index - 1], domain[1])
            } else {
                (bounds[index - 1], bounds[index])
            };

            let e0 = encode[2 * index];
            let e1 = encode[2 * index + 1];

            let t = (x_clamped - b0) / (b1 - b0);
            let x_mapped = e0 + t * (e1 - e0);

            functions[index].interpolate(x_mapped)
        } else {
            Err(FunctionInterpolationError::UnsupportedFunctionType(
                self.function_type,
            ))
        }
    }
}

impl Function {
    pub(crate) fn from_dictionary(
        dictionary: &Dictionary,
        objects: &ObjectCollection,
        stream: Option<&[u8]>,
    ) -> Result<Function, FunctionReadError> {
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
                let domain = if let Some(obj) = dictionary.get("Domain") {
                    obj.as_array_of::<f32, 2>()
                        .map_err(|err| FunctionReadError::DomainParsingError(err))?
                } else {
                    return Err(FunctionReadError::MissingDomain);
                };

                let c0 = dictionary
                    .get("C0")
                    .unwrap()
                    .as_vec_of::<f32>()
                    .map_err(|e| FunctionReadError::EntryReadError {
                        entry_description: "C0",
                        source: e,
                    })?;

                let c1 = dictionary
                    .get("C1")
                    .unwrap()
                    .as_vec_of::<f32>()
                    .map_err(|e| FunctionReadError::EntryReadError {
                        entry_description: "C1",
                        source: e,
                    })?;

                let exponent = dictionary
                    .get("N")
                    .ok_or(FunctionReadError::InvalidFunctionType)?
                    .as_number::<f32>()
                    .map_err(|e| FunctionReadError::NumericConversionError {
                        entry_description: "N",
                        source: e,
                    })?;

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
                let domain = if let Some(obj) = dictionary.get("Domain") {
                    obj.as_array_of::<f32, 2>()
                        .map_err(|err| FunctionReadError::DomainParsingError(err))?
                } else {
                    return Err(FunctionReadError::MissingDomain);
                };

                // Parse Functions array
                let functions_arr = dictionary
                    .get_array("Functions")
                    .ok_or(FunctionReadError::InvalidFunctionType)?;
                let mut functions = Vec::new();
                for obj in functions_arr.iter() {
                    let dict = obj
                        .as_dictionary()
                        .ok_or(FunctionReadError::InvalidFunctionType)?;
                    let func = Function::from_dictionary(dict, objects, None)?;
                    functions.push(func);
                }

                // Parse Bounds array
                let bounds = dictionary
                    .get("Bounds")
                    .unwrap()
                    .as_vec_of::<f32>()
                    .map_err(|e| FunctionReadError::EntryReadError {
                        entry_description: "Bounds",
                        source: e,
                    })?;

                // Parse Encode array
                let encode = dictionary
                    .get("Encode")
                    .unwrap()
                    .as_vec_of::<f32>()
                    .map_err(|e| FunctionReadError::EntryReadError {
                        entry_description: "Encode",
                        source: e,
                    })?;

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
            FunctionType::PostScriptCalculator => {
                let stream = stream.ok_or(FunctionReadError::InvalidFunctionType)?;
                println!("PostScriptCalculator: {}", String::from_utf8_lossy(stream));

                todo!()
            }
            _ => Err(FunctionReadError::InvalidFunctionType),
        }
    }
}
