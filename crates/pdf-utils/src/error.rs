use thiserror::Error;

/// Errors that can occur while reading byte-aligned values from a bit stream.
#[non_exhaustive]
#[derive(Debug, Error, Clone, PartialEq)]
pub enum BitReaderError {
    /// The stream ended before all requested bytes were available.
    #[error("stream truncated: {0}")]
    Truncated(&'static str),

    /// Integer conversion failed because the destination type was too small.
    #[error("integer conversion overflow: {0}")]
    Overflow(&'static str),
}
