//! PDF Function objects for color space transformations and interpolation.
//!
//! This module implements the PDF function types as defined in the PDF specification:
//! - Type 0: Sampled functions (lookup table with interpolation)
//! - Type 2: Exponential Interpolation functions
//! - Type 3: Stitching functions (combining multiple functions)
//! - Type 4: PostScript Calculator functions

use pdf_object::{
    object_lookup::ObjectLookupExt, object_resolver::ObjectResolver, object_variant::ObjectVariant,
};

use crate::{
    error::FunctionReadError, exponential_interpolation::ExponentialFunction,
    function_interpolation_error::FunctionInterpolationError,
    postscript_calculator::PostScriptCalculatorFunction, sampled::SampledFunction,
    stitching::StitchingFunction,
};

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

pub trait FunctionImpl {
    /// Interpolates a slice of input values according to the function's definition.
    ///
    /// Each input is clamped to its corresponding domain entry before interpolation.
    /// Output values are clamped to any defined range. The slice must contain at
    /// least as many values as the function has input dimensions.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Fewer inputs are provided than the function requires
    /// - The function data is structurally invalid
    /// - A PostScript calculation fails
    fn interpolate(&self, inputs: &[f32]) -> Result<Vec<f32>, FunctionInterpolationError>;

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
    fn interpolate(&self, inputs: &[f32]) -> Result<Vec<f32>, FunctionInterpolationError> {
        match self {
            Function::Sampled(f) => f.interpolate(inputs),
            Function::Exponential(f) => f.interpolate(inputs),
            Function::Stitching(f) => f.interpolate(inputs),
            Function::PostScriptCalculator(f) => f.interpolate(inputs),
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
            .required_number::<i32>(b"FunctionType", objects)
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

impl Function {
    /// Evaluates the function for one or more input values.
    ///
    /// `inputs` must contain at least as many values as the function has input
    /// dimensions. Extra values are ignored. Each input is clamped to its domain.
    pub fn apply(&self, inputs: &[f32]) -> Result<Vec<f32>, FunctionInterpolationError> {
        FunctionImpl::interpolate(self, inputs)
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
