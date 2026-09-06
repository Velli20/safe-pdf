//! Error types produced while decoding PDF sample data.

use thiserror::Error;

use pdf_object_reader::object_error::ObjectError;

/// Errors returned by the PDF decode helpers in this crate.
#[derive(Debug, Error)]
pub enum DecodeError {
    /// Wraps a lower-level object parsing or resolution error.
    #[error("{0}")]
    Object(#[from] ObjectError),
    /// Reports an unsupported `BitsPerSample` value.
    #[error("unsupported bits per sample value: {bits_per_sample}")]
    InvalidBitsPerSample {
        /// The unsupported bit width that was requested.
        bits_per_sample: usize,
    },
    /// Reports that the input ended before all expected bytes were available.
    #[error(
        "sample data is truncated: expected at least {expected_bytes} bytes, got {actual_bytes}"
    )]
    InsufficientData {
        /// The minimum number of bytes required by the decode operation.
        expected_bytes: usize,
        /// The number of bytes actually present in the input.
        actual_bytes: usize,
    },
    /// Reports that a `/Decode` array has the wrong number of values.
    #[error("invalid /Decode array length: expected {expected_values} values, got {actual_values}")]
    InvalidDecodeLength {
        /// The number of values expected for the current component count.
        expected_values: usize,
        /// The number of values actually found in the array.
        actual_values: usize,
    },
    /// Reports that a `/Decode` value could not be converted to a finite number.
    #[error("invalid /Decode value")]
    InvalidDecodeValue,
    /// Reports that an indexed palette does not define any base components.
    #[error("palette base component count must be non-zero")]
    InvalidComponentCount,
    /// Reports that a palette lookup would read past the end of the lookup table.
    #[error(
        "palette index {index} out of bounds at pixel {pixel_index} (lookup table size: {lookup_len})"
    )]
    PaletteLookupOutOfBounds {
        /// The palette index that was requested.
        index: u8,
        /// The zero-based pixel position where the lookup failed.
        pixel_index: usize,
        /// The size of the lookup table in bytes.
        lookup_len: usize,
    },
    /// Reports that the sample data could not be converted safely.
    #[error("sample data conversion failed")]
    InvalidSampleData,
}
