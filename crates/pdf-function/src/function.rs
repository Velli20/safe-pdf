//! PDF Function objects for color space transformations and interpolation.
//!
//! This module implements the PDF function types as defined in the PDF specification:
//! - Type 0: Sampled functions (lookup table with interpolation)
//! - Type 2: Exponential Interpolation functions
//! - Type 3: Stitching functions (combining multiple functions)
//! - Type 4: PostScript Calculator functions

use pdf_graphics::interval::Interval;
use pdf_object_reader::{FromPdfObject, ObjectAccess, ObjectContext, ReadResult};

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

/// Reads the `/FunctionType` entry of a function dictionary.
impl FromPdfObject for FunctionType {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        match context.decode::<i32>()? {
            0 => Ok(Self::Sampled),
            2 => Ok(Self::ExponentialInterpolation),
            3 => Ok(Self::Stitching),
            4 => Ok(Self::PostScriptCalculator),
            _ => Err(FunctionReadError::InvalidFunctionType.into()),
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

    /// Returns the input domain of this function's first input dimension.
    fn domain(&self) -> Option<Interval>;
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

    fn domain(&self) -> Option<Interval> {
        match self {
            Function::Sampled(function) => function.domain(),
            Function::Exponential(function) => function.domain(),
            Function::Stitching(function) => function.domain(),
            Function::PostScriptCalculator(function) => function.domain(),
        }
    }
}

/// Reads a function dictionary or stream, dispatching on its `/FunctionType`.
impl FromPdfObject for Function {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let object = context.object();
        let mut context = context.dictionary()?;
        match context.required::<FunctionType>(b"FunctionType")? {
            FunctionType::Sampled => context.read(object).map(Function::Sampled),
            FunctionType::ExponentialInterpolation => {
                context.read(object).map(Function::Exponential)
            }
            FunctionType::Stitching => context.read(object).map(Function::Stitching),
            FunctionType::PostScriptCalculator => {
                context.read(object).map(Function::PostScriptCalculator)
            }
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
