use pdf_postscript::calculator::CalcError;
use thiserror::Error;

use crate::function::FunctionType;

/// Errors that can occur during function interpolation.
#[derive(Debug, Error)]
pub enum FunctionInterpolationError {
    #[error("interpolation is not implemented for {0:?} functions")]
    UnsupportedFunctionType(FunctionType),
    #[error("/Domain must define a strictly increasing interval [min, max]")]
    InvalidDomainInterval,
    #[error("PostScript calculator error: {0}")]
    PostScriptCalculatorError(#[from] CalcError),
    #[error("/Encode must contain exactly two values for each stitched sub-function")]
    InvalidEncodeLength,
    #[error("/Bounds must contain exactly one fewer value than /Functions")]
    InvalidBoundsLength,
    #[error("function data is internally inconsistent: indexed lookup was out of bounds")]
    FunctionDataIndexOutOfBounds,
    #[error("PostScript function returned fewer values than required by /Range")]
    PostScriptResultStackUnderflow,
    #[error("cannot evaluate 0 raised to a negative exponent")]
    UndefinedExponentiationAtZero,
    #[error("function produced a non-finite numeric value")]
    NonFiniteNumericValue,
    #[error("PostScript function returned a non-numeric value")]
    NonNumericPostScriptOutput,
    #[error("sample lookup produced an out-of-bounds coordinate or offset")]
    SampleCoordinateOutOfBounds,
    #[error("sample data is invalid or cannot be decoded")]
    InvalidSampleData,
    #[error("function returned too few color components: expected {required}, got {returned}")]
    ColorComponentCountMismatch { required: usize, returned: usize },
    #[error("indexed color spaces are not supported for function color conversion")]
    IndexedColorSpaceNotSupported,
    /// Cubic spline interpolation for Type 0 functions is not implemented.
    #[error("sampled functions with /Order 3 are recognized but not implemented")]
    CubicInterpolationUnsupported,
    /// Caller provided fewer inputs than the function requires.
    #[error("function requires {expected} input value(s), got {got}")]
    InsufficientInputs { expected: usize, got: usize },
    /// A bounds value in a stitching function is NaN.
    #[error("/Bounds contains NaN, so the stitching segment cannot be determined")]
    BoundsContainNaN,
}
