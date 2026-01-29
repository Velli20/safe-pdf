//! PDF Function objects for color space transformations and interpolation.
//!
//! This module implements the PDF function types as defined in the PDF specification:
//! - Type 0: Sampled functions (lookup table with interpolation)
//! - Type 2: Exponential Interpolation functions
//! - Type 3: Stitching functions (combining multiple functions)
//! - Type 4: PostScript Calculator functions

use pdf_object::{ObjectVariant, error::ObjectError, object_resolver::ObjectResolver};
use pdf_postscript::calculator::CalcError;
use thiserror::Error;

use crate::functions::{
    exponential_interpolation::ExponentialFunction,
    postscript_calculator::PostScriptCalculatorFunction, sampled::SampledFunction,
    stitching::StitchingFunction,
};

pub mod exponential_interpolation;
pub mod postscript_calculator;
pub mod sampled;
pub mod stitching;

/// Errors that can occur when parsing a PDF Function dictionary.
#[derive(Debug, Error)]
pub enum FunctionReadError {
    /// The `/FunctionType` entry is missing or has an unsupported value.
    #[error("Invalid /FunctionType value")]
    InvalidFunctionType,
    /// The function requires an associated stream but none was provided.
    #[error("Stream data is required for {function_type:?} functions")]
    StreamRequired { function_type: FunctionType },
    /// The `/Encode` array length must be exactly `2 * number of functions`.
    #[error("Encode array length must be exactly 2 * number of functions")]
    InvalidEncodeLength,
    /// The `/Bounds` array length must be `number of functions - 1`.
    #[error("Bounds array length must be number of functions - 1")]
    InvalidBoundsLength,
    /// A function entry was not a dictionary or stream.
    #[error("Function entry must be a dictionary or stream")]
    InvalidFunctionEntryType,
    /// Failed to read a required dictionary entry.
    #[error("Failed to read function value for '{entry_description}': {source}")]
    EntryReadError {
        entry_description: &'static str,
        #[source]
        source: ObjectError,
    },
    /// Failed to parse the `/Domain` array.
    #[error("Domain parsing error: {0}")]
    DomainParsingError(#[from] ObjectError),
    /// Error during PostScript code parsing.
    #[error("PostScript calculator error: {0}")]
    PostScriptCalculatorError(#[from] CalcError),
    /// The `/Size` array is empty or invalid for a sampled function.
    #[error("Size array must have at least one element")]
    InvalidSizeArray,
    /// The `/BitsPerSample` value is invalid.
    #[error("BitsPerSample must be 1, 2, 4, 8, 12, 16, 24, or 32")]
    InvalidBitsPerSample,
    /// The stream data is too short for the declared sample table.
    #[error(
        "Stream data is too short for the sample table (expected {expected} bytes, got {actual})"
    )]
    InsufficientStreamData { expected: usize, actual: usize },
    /// The `/Order` value is invalid (must be 1 or 3).
    #[error("Order must be 1 (linear) or 3 (cubic)")]
    InvalidOrder,
    /// The `/Decode` array length is invalid.
    #[error("Decode array length must be exactly 2 * number of outputs")]
    InvalidDecodeLength,
}

/// Errors that can occur during function interpolation.
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
    #[error("Sample index out of bounds")]
    SampleIndexOutOfBounds,
    #[error("Invalid sample data")]
    InvalidSampleData,
    #[error(
        "Function returned insufficient color components: required {required}, returned {returned}"
    )]
    InsufficientColorComponents { required: usize, returned: usize },
    #[error("Indexed color space is unsupported for color conversion")]
    IndexedColorSpaceUnsupported,
}

/// Represents the type of a PDF Function object (Table 38 in PDF spec).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionType {
    /// Type 0: A sampled function using a table of sample values.
    Sampled = 0,
    /// Type 2: An exponential interpolation function.
    ExponentialInterpolation = 2,
    /// Type 3: A stitching function combining multiple sub-functions.
    Stitching = 3,
    /// Type 4: A PostScript calculator function.
    PostScriptCalculator = 4,
}

impl FunctionType {
    /// Creates a `FunctionType` from an integer value.
    ///
    /// # Returns
    ///
    /// `Some(FunctionType)` if the value corresponds to a supported function type,
    /// `None` otherwise.
    pub fn from_i32(val: i32) -> Option<Self> {
        match val {
            0 => Some(Self::Sampled),
            2 => Some(Self::ExponentialInterpolation),
            3 => Some(Self::Stitching),
            4 => Some(Self::PostScriptCalculator),
            _ => None,
        }
    }
}

pub(crate) trait FunctionImpl {
    /// Interpolates an input value `x` according to the function's definition.
    ///
    /// The input is automatically clamped to the function's domain before
    /// interpolation. The output values are also clamped to any defined range.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The input is NaN
    /// - The function data is structurally invalid
    /// - A PostScript calculation fails
    fn interpolate(&self, x: f32) -> Result<Vec<f32>, FunctionInterpolationError>;

    /// Returns the input domain of this function as `[min, max]`.
    fn domain(&self) -> Option<[f32; 2]>;

    fn parse(
        object: &ObjectVariant,
        objects: &dyn ObjectResolver,
    ) -> Result<Function, FunctionReadError>;
}

/// A PDF Function object for mapping input values to output values.
///
/// Functions are used extensively in PDF for color space conversions,
/// shading patterns, and other transformations.
#[derive(Debug, Clone)]
pub enum Function {
    /// Data for Type 0 (Sampled) functions.
    Sampled(SampledFunction),
    /// Data for Type 2 (Exponential Interpolation) functions.
    Exponential(ExponentialFunction),
    /// Data for Type 3 (Stitching) functions.
    Stitching(StitchingFunction),
    /// Data for Type 4 (PostScript Calculator) functions.
    PostScriptCalculator(PostScriptCalculatorFunction),
}

impl FunctionImpl for Function {
    fn interpolate(&self, x: f32) -> Result<Vec<f32>, FunctionInterpolationError> {
        match self {
            Function::Sampled(f) => f.interpolate(x),
            Function::Exponential(f) => f.interpolate(x),
            Function::Stitching(f) => f.interpolate(x),
            Function::PostScriptCalculator(f) => f.interpolate(x),
        }
    }

    fn domain(&self) -> Option<[f32; 2]> {
        match self {
            Function::Sampled(function) => function.domain(),
            Function::Exponential(function) => function.domain(),
            Function::Stitching(function) => function.domain(),
            Function::PostScriptCalculator(function) => function.domain(),
        }
    }

    /// Parses a PDF Function object from a dictionary.
    ///
    /// # Parameters
    ///
    /// - `dictionary`: The function dictionary containing function parameters.
    /// - `objects`: The object collection for resolving indirect references.
    /// - `stream`: Optional stream data (required for Type 4 PostScript functions).
    ///
    /// # Errors
    ///
    /// Returns an error if the dictionary is malformed or contains invalid values.
    fn parse(
        object: &ObjectVariant,
        objects: &dyn ObjectResolver,
    ) -> Result<Function, FunctionReadError> {
        let function_type = object
            .try_dictionary(objects)?
            .get_or_err("FunctionType")?
            .try_number::<i32>(objects)
            .map(FunctionType::from_i32)?
            .ok_or(FunctionReadError::InvalidFunctionType)?;

        match function_type {
            FunctionType::ExponentialInterpolation => ExponentialFunction::parse(object, objects),
            FunctionType::Stitching => StitchingFunction::parse(object, objects),
            FunctionType::PostScriptCalculator => {
                PostScriptCalculatorFunction::parse(object, objects)
            }
            FunctionType::Sampled => SampledFunction::parse(object, objects),
        }
    }
}

/// Clamps a value to a domain and returns the normalized position in `[0, 1]`.
///
/// Returns `None` if the domain is degenerate (min >= max).
#[inline]
pub fn clamp_and_normalize(x: f32, domain: [f32; 2]) -> Option<f32> {
    let [min, max] = domain;
    if min >= max {
        return None;
    }
    let clamped = x.clamp(min, max);
    Some((clamped - min) / (max - min))
}

/// Performs linear interpolation from `[a, b]` to `[c, d]`.
///
/// Maps the position of `x` within `[a, b]` to the corresponding position in `[c, d]`.
#[inline]
pub fn linear_interpolate(x: f32, a: f32, b: f32, c: f32, d: f32) -> f32 {
    let t = if (b - a).abs() < f32::EPSILON {
        0.0
    } else {
        (x - a) / (b - a)
    };
    c + t * (d - c)
}

/// Safely retrieves a pair of values at index `i` from a slice of pairs.
///
/// For a slice `[v0, v1, v2, v3, ...]`, returns `(slice[2*i], slice[2*i+1])`.
#[inline]
pub fn get_pair(slice: &[f32], i: usize) -> Option<(f32, f32)> {
    let base = i.checked_mul(2)?;
    let first = *slice.get(base)?;
    let second = *slice.get(base.checked_add(1)?)?;
    Some((first, second))
}

/// Ensures the underlying stream contains at least `expected` bytes.
#[inline]
pub fn ensure_stream_len(stream: &[u8], expected: usize) -> Result<(), FunctionReadError> {
    if stream.len() < expected {
        Err(FunctionReadError::InsufficientStreamData {
            expected,
            actual: stream.len(),
        })
    } else {
        Ok(())
    }
}
