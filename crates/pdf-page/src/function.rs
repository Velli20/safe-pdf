use pdf_object::{dictionary::Dictionary, error::ObjectError, object_collection::ObjectCollection};

use pdf_postscript::{calculator::CalcError, operator::Operator};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum FunctionReadError {
    #[error("Missing /FunctionType entry")]
    MissingFunctionType,
    #[error("Missing /Domain entry")]
    MissingDomain,
    #[error("Missing /Range entry")]
    MissingRange,
    #[error("Missing required entry in Function: /{0}")]
    MissingRequiredEntry(&'static str),
    #[error("Invalid /FunctionType value")]
    InvalidFunctionType,
    #[error("Failed to convert PDF value to number for '{entry_description}': {source}")]
    NumericConversionError {
        entry_description: &'static str,
        #[source]
        source: ObjectError,
    },
    #[error(
        "Entry '{entry_name}' in Shading dictionary has invalid type: expected {expected_type}, found {found_type}"
    )]
    InvalidEntryType {
        entry_name: &'static str,
        expected_type: &'static str,
        found_type: &'static str,
    },
    #[error("Failed to read function value for '{entry_description}': {source}")]
    EntryReadError {
        entry_description: &'static str,
        #[source]
        source: ObjectError,
    },
    #[error("Domain parsing error: {0}")]
    DomainParsingError(#[from] ObjectError),
    #[error("PostScript calculator error: {0}")]
    PostScriptCalculatorError(#[from] CalcError),
}

#[derive(Debug, Error)]
pub enum FunctionInterpolationError {
    #[error("Interpolation is not supported for function type {0:?}")]
    UnsupportedFunctionType(FunctionType),
    #[error("C0 and C1 arrays must have the same length")]
    MismatchedC0C1Length,
    #[error("Domain must be an increasing interval (domain[0] < domain[1])")]
    InvalidDomain,
    #[error("PostScript calculator error: {0}")]
    PostScriptCalculatorError(#[from] CalcError),
    #[error("Encode array length must be exactly 2 * number of functions")]
    InvalidEncodeLength,
    #[error("Bounds array length must be number of functions - 1")]
    InvalidBoundsLength,
    #[error("Index calculation overflow or out-of-bounds during encode access")]
    EncodeIndexError,
    #[error("Result stack does not contain enough values for declared range")]
    InsufficientResultStack,
    #[error("Negative exponent with zero normalized input produces undefined result")]
    NegativeExponentAtZero,
    #[error("Input value is NaN")]
    InputIsNaN,
}

/// Represents the type of a PDF Function object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionType {
    /// A sampled function that uses a table of sample values to define the function.
    Sampled = 0,
    /// An exponential interpolation function.
    ExponentialInterpolation = 2,
    /// A stitching function that combines several other functions into a single function.
    Stitching = 3,
    /// A PostScript calculator function that uses a small subset of the PostScript language
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
enum FunctionData {
    Exponential {
        c0: Vec<f32>,
        c1: Vec<f32>,
        exponent: f32,
        domain: [f32; 2],
    },
    Stitching {
        functions: Vec<Function>,
        bounds: Vec<f32>,
        encode: Vec<f32>,
        domain: [f32; 2],
    },
    PostScriptCalculator {
        operators: Vec<Operator>,
        domain: Vec<f32>,
        range: Vec<f32>,
    },
}

#[derive(Debug)]
pub struct Function {
    pub function_type: FunctionType,
    data: FunctionData,
}

impl Function {
    pub fn domain(&self) -> Option<[f32; 2]> {
        match &self.data {
            FunctionData::Exponential { domain, .. } => Some(*domain),
            FunctionData::Stitching { domain, .. } => Some(*domain),
            FunctionData::PostScriptCalculator { domain, .. } => {
                if domain.len() >= 2 {
                    Some([domain[0], domain[1]])
                } else {
                    None
                }
            }
        }
    }

    #[inline]
    fn safe_f64_to_f32(val: f64) -> f32 {
        // Avoid direct casts in comparisons flagged by clippy; manual clamping then single cast.
        if val.is_nan() {
            return f32::NAN;
        }
        const F32_MAX_F64: f64 = 3.4028234663852886e38_f64; // f32::MAX as f64
        const F32_MIN_F64: f64 = -3.4028234663852886e38_f64; // f32::MIN as f64
        let clamped = val.clamp(F32_MIN_F64, F32_MAX_F64);
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_precision_loss,
            clippy::as_conversions
        )]
        {
            clamped as f32
        }
    }

    /// Interpolates an input value `x` according to the function's definition.
    ///
    /// # Returns
    ///
    /// A `Result` containing a `Vec<f32>` of the output values on success, or a
    /// `FunctionInterpolationError` on failure.
    pub fn interpolate(&self, x: f32) -> Result<Vec<f32>, FunctionInterpolationError> {
        if x.is_nan() {
            return Err(FunctionInterpolationError::InputIsNaN);
        }
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
            // 2. Normalize the clipped value to the interval [0, 1]. (explicit safe arithmetic)
            let denom = domain[1] - domain[0];
            debug_assert!(denom > 0.0);
            let x_normalized = (x_clipped - domain[0]) / denom;

            // Guard: if exponent is negative and x_normalized == 0 => undefined (division by zero)
            if *exponent < 0.0 && x_normalized == 0.0 {
                return Err(FunctionInterpolationError::NegativeExponentAtZero);
            }

            // 3. Apply the interpolation formula for each component using checked powf.
            let pow = x_normalized.powf(*exponent); // For f32 this won't panic, guard above handles undefined case.
            let result = c0
                .iter()
                .zip(c1.iter())
                .map(|(&c0_i, &c1_i)| c0_i + pow * (c1_i - c0_i))
                .collect();

            Ok(result)
        } else if let FunctionData::Stitching {
            functions,
            bounds,
            encode,
            domain,
        } = &self.data
        {
            // 1. Clamp the input value `x` to the function's domain.
            let x_clamped = x.clamp(domain[0], domain[1]);

            // Validate structural invariants once (could be done at construction but cheap here)
            if functions.len().checked_sub(1) != Some(bounds.len()) {
                return Err(FunctionInterpolationError::InvalidBoundsLength);
            }
            if functions.len().checked_mul(2).map(|v| v == encode.len()) != Some(true) {
                return Err(FunctionInterpolationError::InvalidEncodeLength);
            }

            // 2. Find which subfunction to use based on the `bounds` array.
            // The `bounds` array divides the domain into sub-domains, each corresponding
            // to a function in the `functions` array.
            // Use binary search for clearer intent & performance.
            use core::cmp::Ordering;
            let index = match bounds.binary_search_by(|b| {
                // Treat NaN as Less to keep index at 0 (already guarded earlier for x NaN)
                b.partial_cmp(&x_clamped).unwrap_or(Ordering::Less)
            }) {
                Ok(pos) => pos.saturating_add(1), // If exactly equal to a bound, move to the next segment
                Err(pos) => pos,
            };

            // 3. Determine the input range (sub-domain) for the selected subfunction.
            // This is the interval [b0, b1] that contains `x_clamped`.
            let (b0, b1) = if index == 0 {
                (domain[0], bounds[0])
            } else if index == bounds.len() {
                let prev_index = index
                    .checked_sub(1)
                    .ok_or(FunctionInterpolationError::EncodeIndexError)?;
                let prev = *bounds
                    .get(prev_index)
                    .ok_or(FunctionInterpolationError::EncodeIndexError)?;
                (prev, domain[1])
            } else {
                let prev_index = index
                    .checked_sub(1)
                    .ok_or(FunctionInterpolationError::EncodeIndexError)?;
                let prev = *bounds
                    .get(prev_index)
                    .ok_or(FunctionInterpolationError::EncodeIndexError)?;
                let current = *bounds
                    .get(index)
                    .ok_or(FunctionInterpolationError::EncodeIndexError)?;
                (prev, current)
            };

            // 4. Get the encoding values for the selected subfunction.
            // The `encode` array defines how to map the value from the sub-domain [b0, b1]
            // to the subfunction's own input domain [e0, e1].
            let enc_base = index
                .checked_mul(2)
                .ok_or(FunctionInterpolationError::EncodeIndexError)?;
            let e0 = *encode
                .get(enc_base)
                .ok_or(FunctionInterpolationError::EncodeIndexError)?;
            let e1 = *encode
                .get(
                    enc_base
                        .checked_add(1)
                        .ok_or(FunctionInterpolationError::EncodeIndexError)?,
                )
                .ok_or(FunctionInterpolationError::EncodeIndexError)?;

            // 5. Linearly interpolate the clamped input value from the sub-domain [b0, b1]
            // to the subfunction's domain [e0, e1].
            // First, calculate the normalized position `t` of `x_clamped` within [b0, b1].
            // Handle potential division by zero if the sub-domain has zero width.
            let t = if (b1 - b0).abs() < f32::EPSILON {
                0.0
            } else {
                (x_clamped - b0) / (b1 - b0)
            };
            // Then, map `t` to the target range [e0, e1].
            let x_mapped = e0 + t * (e1 - e0);

            // 6. Call the selected subfunction with the mapped input value.
            functions[index].interpolate(x_mapped)
        } else if let FunctionData::PostScriptCalculator {
            operators,
            domain,
            range,
        } = &self.data
        {
            // 1. Clip input to domain
            let mut stack = Vec::new();
            let input_count = domain.len() / 2;
            if input_count == 1 {
                let x_clipped = x.max(domain[0]).min(domain[1]);
                stack.push(f64::from(x_clipped));
            } else {
                // If you want to support multi-input, change interpolate signature to accept &[f32]
                // For now, treat x as the first input and fill others with domain min
                for i in 0..input_count {
                    let val = if i == 0 {
                        x.max(domain[0]).min(domain[1])
                    } else {
                        let pair_index = i
                            .checked_mul(2)
                            .ok_or(FunctionInterpolationError::EncodeIndexError)?;
                        let start = *domain
                            .get(pair_index)
                            .ok_or(FunctionInterpolationError::EncodeIndexError)?;
                        let end = *domain
                            .get(
                                pair_index
                                    .checked_add(1)
                                    .ok_or(FunctionInterpolationError::EncodeIndexError)?,
                            )
                            .ok_or(FunctionInterpolationError::EncodeIndexError)?;
                        start.max(start).min(end)
                    };
                    stack.push(f64::from(val));
                }
            }

            // 2. Evaluate PostScript operators
            let result_stack = pdf_postscript::calculator::execute(&stack, operators)?;

            // 3. Clip outputs to range
            let mut outputs = Vec::new();
            let output_count = range.len() / 2;
            for i in 0..output_count {
                let val = result_stack
                    .get(i)
                    .ok_or(FunctionInterpolationError::InsufficientResultStack)?;
                let base = i
                    .checked_mul(2)
                    .ok_or(FunctionInterpolationError::EncodeIndexError)?;
                let min = *range
                    .get(base)
                    .ok_or(FunctionInterpolationError::EncodeIndexError)?;
                let max = *range
                    .get(
                        base.checked_add(1)
                            .ok_or(FunctionInterpolationError::EncodeIndexError)?,
                    )
                    .ok_or(FunctionInterpolationError::EncodeIndexError)?;
                let v_f64 = *val;
                let mut v_f32 = Self::safe_f64_to_f32(v_f64);
                v_f32 = v_f32.max(min).min(max);
                outputs.push(v_f32);
            }
            Ok(outputs)
        } else {
            unreachable!();
        }
    }
}

impl Function {
    pub(crate) fn from_dictionary(
        dictionary: &Dictionary,
        _objects: &ObjectCollection,
        stream: Option<&[u8]>,
    ) -> Result<Function, FunctionReadError> {
        let function_type_int = dictionary
            .get("FunctionType")
            .ok_or(FunctionReadError::MissingFunctionType)?
            .as_number::<i32>()?;

        let function_type = FunctionType::from_i32(function_type_int)
            .ok_or(FunctionReadError::InvalidFunctionType)?;

        match function_type {
            FunctionType::ExponentialInterpolation => {
                let domain = if let Some(obj) = dictionary.get("Domain") {
                    obj.as_array_of::<f32, 2>()
                        .map_err(FunctionReadError::DomainParsingError)?
                } else {
                    return Err(FunctionReadError::MissingDomain);
                };

                // Parse /C0, the function's output for domain[0]. Defaults to [0.0] if not present.
                let c0 = if let Some(obj) = dictionary.get("C0") {
                    obj.as_vec_of::<f32>()
                        .map_err(|e| FunctionReadError::EntryReadError {
                            entry_description: "C0",
                            source: e,
                        })?
                } else {
                    vec![0.0]
                };

                // Parse /C1, the function's output for domain[1]. Defaults to [1.0] if not present.
                let c1 = if let Some(obj) = dictionary.get("C1") {
                    obj.as_vec_of::<f32>()
                        .map_err(|e| FunctionReadError::EntryReadError {
                            entry_description: "C1",
                            source: e,
                        })?
                } else {
                    vec![1.0]
                };

                // Parse /N, the interpolation exponent (required).
                let exponent = dictionary
                    .get("N")
                    .ok_or(FunctionReadError::MissingRequiredEntry("N"))?
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
                        .map_err(FunctionReadError::DomainParsingError)?
                } else {
                    return Err(FunctionReadError::MissingDomain);
                };

                // Parse Functions array
                let functions_arr = dictionary
                    .get_array("Functions")
                    .ok_or(FunctionReadError::MissingRequiredEntry("Functions"))?;

                let mut functions = Vec::new();
                for obj in functions_arr.iter() {
                    let dict =
                        obj.as_dictionary()
                            .ok_or_else(|| FunctionReadError::InvalidEntryType {
                                entry_name: "Functions",
                                expected_type: "Dictionary",
                                found_type: obj.name(),
                            })?;
                    functions.push(Function::from_dictionary(dict, _objects, None)?);
                }

                // Parse Bounds array
                let bounds = dictionary
                    .get("Bounds")
                    .ok_or(FunctionReadError::MissingRequiredEntry("Bounds"))?
                    .as_vec_of::<f32>()
                    .map_err(|e| FunctionReadError::EntryReadError {
                        entry_description: "Bounds",
                        source: e,
                    })?;

                // Parse Encode array
                let encode = dictionary
                    .get("Encode")
                    .ok_or(FunctionReadError::MissingRequiredEntry("Encode"))?
                    .as_vec_of::<f32>()
                    .map_err(|e| FunctionReadError::EntryReadError {
                        entry_description: "Encode",
                        source: e,
                    })?;

                // Validate relationships to avoid later arithmetic surprises
                if functions.len().checked_sub(1) != Some(bounds.len()) {
                    return Err(FunctionReadError::MissingRequiredEntry(
                        "Bounds / Functions length mismatch",
                    ));
                }
                if functions.len().checked_mul(2).map(|v| v == encode.len()) != Some(true) {
                    return Err(FunctionReadError::MissingRequiredEntry(
                        "Encode length invalid",
                    ));
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
            FunctionType::PostScriptCalculator => {
                let domain = if let Some(obj) = dictionary.get("Domain") {
                    obj.as_vec_of::<f32>()
                        .map_err(FunctionReadError::DomainParsingError)?
                } else {
                    return Err(FunctionReadError::MissingDomain);
                };

                let range = if let Some(obj) = dictionary.get("Range") {
                    obj.as_vec_of::<f32>()
                        .map_err(FunctionReadError::DomainParsingError)?
                } else {
                    return Err(FunctionReadError::MissingRange);
                };

                let stream = stream.ok_or(FunctionReadError::InvalidFunctionType)?;
                let a = String::from_utf8_lossy(stream);
                let code = a.replace("{", " { ").replace("}", " } ");
                Ok(Function {
                    function_type,
                    data: FunctionData::PostScriptCalculator {
                        operators: pdf_postscript::calculator::parse_tokens(
                            &code.split_whitespace().collect::<Vec<_>>(),
                        )?,
                        domain,
                        range,
                    },
                })
            }
            _ => Err(FunctionReadError::InvalidFunctionType),
        }
    }
}
