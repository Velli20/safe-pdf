use thiserror::Error;

use pdf_ccitt::CcittDecodeError;

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

    /// The stream ended before all required data was available.
    #[error("stream truncated: {0}")]
    Truncated(&'static str),

    /// Integer conversion failed because the destination type was too small.
    #[error("integer conversion overflow: {0}")]
    Overflow(&'static str),

    /// CCITT fax decoding failed.
    #[error(transparent)]
    CcittDecode(#[from] CcittDecodeError),

    /// A PDF object-level error occurred while reading filter metadata
    /// (e.g., resolving the `/Filter` or `/DecodeParms` entries).
    #[error("object error: {0}")]
    Object(String),
}

impl From<pdf_jbig2::Jbig2Error> for FilterError {
    fn from(err: pdf_jbig2::Jbig2Error) -> Self {
        Self::Decompression(err.to_string())
    }
}

impl From<pdf_utils::BitReaderError> for FilterError {
    fn from(err: pdf_utils::BitReaderError) -> Self {
        match err {
            pdf_utils::BitReaderError::Truncated(message) => Self::Truncated(message),
            pdf_utils::BitReaderError::Overflow(message) => Self::Overflow(message),
            _ => Self::Decompression(err.to_string()),
        }
    }
}

impl From<pdf_object_reader::object_error::ObjectError> for FilterError {
    fn from(err: pdf_object_reader::object_error::ObjectError) -> Self {
        Self::Object(err.to_string())
    }
}

impl From<FilterError> for pdf_object_reader::object_error::ObjectError {
    fn from(err: FilterError) -> Self {
        match err {
            FilterError::Decompression(msg) => Self::DecompressionError(msg),
            FilterError::UnsupportedFilter(name) => Self::UnsupportedFilter(name),
            FilterError::Truncated(msg) => Self::DecompressionError(msg.to_string()),
            FilterError::Overflow(msg) => Self::DecompressionError(msg.to_string()),
            FilterError::CcittDecode(e) => Self::DecompressionError(e.to_string()),
            FilterError::Object(msg) => Self::DecompressionError(msg),
        }
    }
}
