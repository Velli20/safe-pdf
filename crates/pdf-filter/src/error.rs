use thiserror::Error;

use crate::ccitt::CcittDecodeError;

/// Errors that can occur during PDF stream filter decoding.
///
/// Each variant captures a specific failure mode so callers can decide how
/// to handle partial or corrupted data.
#[non_exhaustive]
#[derive(Debug, Error, Clone, PartialEq)]
pub enum FilterError {
    /// The stream data could not be decompressed.
    ///
    /// This covers failures from zlib (FlateDecode), JPEG (DCTDecode),
    /// JPEG 2000 (JPXDecode), ASCII85Decode, and ASCIIHexDecode decoders.
    #[error("decompression failed: {0}")]
    Decompression(String),

    /// The stream uses a filter that this implementation does not support.
    #[error("unsupported stream filter: {0}")]
    UnsupportedFilter(String),

    /// CCITT fax decoding failed.
    #[error(transparent)]
    CcittDecode(#[from] CcittDecodeError),

    /// A PDF object-level error occurred while reading filter metadata
    /// (e.g., resolving the `/Filter` or `/DecodeParms` entries).
    #[error("object error: {0}")]
    Object(String),
}

impl From<pdf_object::error::ObjectError> for FilterError {
    fn from(err: pdf_object::error::ObjectError) -> Self {
        Self::Object(err.to_string())
    }
}

impl From<FilterError> for pdf_object::error::ObjectError {
    fn from(err: FilterError) -> Self {
        match err {
            FilterError::Decompression(msg) => Self::DecompressionError(msg),
            FilterError::UnsupportedFilter(name) => Self::UnsupportedFilter(name),
            FilterError::CcittDecode(e) => Self::DecompressionError(e.to_string()),
            FilterError::Object(msg) => Self::DecompressionError(msg),
        }
    }
}
