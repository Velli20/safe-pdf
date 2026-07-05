use pdf_decode::DecodeError;
use pdf_object::error::ObjectError;
use pdf_postscript::calculator::CalcError;
use thiserror::Error;

/// Errors that can occur when parsing a PDF Function dictionary.
#[derive(Debug, Error)]
pub enum FunctionReadError {
    /// The `/FunctionType` entry is missing or has an unsupported value.
    #[error("Invalid /FunctionType value")]
    InvalidFunctionType,
    /// The `/Encode` array length must be exactly `2 * number of functions`.
    #[error("Encode array length must be exactly 2 * number of functions")]
    InvalidEncodeLength,
    /// The `/Bounds` array length must be `number of functions - 1`.
    #[error("Bounds array length must be number of functions - 1")]
    InvalidBoundsLength,
    #[error("{0}")]
    ObjectError(#[from] ObjectError),
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
    /// C0 and C1 arrays must have the same length (validated at parse time).
    #[error("C0 and C1 arrays must have the same length")]
    MismatchedC0C1Length,
    /// A sample value could not be converted to the required numeric type.
    #[error("Sample data conversion failed")]
    InvalidSampleData,
}

impl From<DecodeError> for FunctionReadError {
    fn from(value: DecodeError) -> Self {
        match value {
            DecodeError::InvalidBitsPerSample { .. } => Self::InvalidBitsPerSample,
            DecodeError::InsufficientData {
                expected_bytes,
                actual_bytes,
            } => Self::InsufficientStreamData {
                expected: expected_bytes,
                actual: actual_bytes,
            },
            DecodeError::InvalidSampleData => Self::InvalidSampleData,
            DecodeError::Object(err) => Self::ObjectError(err),
            DecodeError::InvalidDecodeLength { .. }
            | DecodeError::InvalidDecodeValue
            | DecodeError::InvalidComponentCount
            | DecodeError::PaletteLookupOutOfBounds { .. } => Self::InvalidSampleData,
        }
    }
}
