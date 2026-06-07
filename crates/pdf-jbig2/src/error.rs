use thiserror::Error;

use pdf_ccitt::CcittDecodeError;
use pdf_utils::BitReaderError;

/// Errors that can occur while decoding JBIG2 streams.
#[non_exhaustive]
#[derive(Debug, Error, Clone, PartialEq)]
pub enum Jbig2Error {
    /// The stream ended before all required data was available.
    #[error("JBIG2Decode: truncated {0}")]
    Truncated(&'static str),

    /// The stream uses a JBIG2 feature this decoder does not support.
    #[error("JBIG2Decode: unsupported {0}")]
    UnsupportedFeature(&'static str),

    /// The stream uses a segment type this decoder does not understand.
    #[error("JBIG2Decode: unsupported segment type {0}")]
    UnsupportedSegmentType(u8),

    /// A referenced table or other decoder input was structurally invalid.
    #[error("JBIG2Decode: invalid {0}")]
    InvalidTable(&'static str),

    /// The stream state is internally inconsistent.
    #[error("JBIG2Decode: invalid {0}")]
    InvalidState(&'static str),

    /// A size or integer conversion overflowed.
    #[error("JBIG2Decode: {0}")]
    Overflow(&'static str),

    /// Memory for decoder state could not be reserved.
    #[error("JBIG2Decode: allocation failed for {0}")]
    Allocation(&'static str),

    /// A referenced segment was not present in the current stream state.
    #[error("JBIG2Decode: missing referenced segment")]
    MissingSegment,

    /// A referenced symbol bitmap was not available.
    #[error("JBIG2Decode: missing {0} symbol")]
    MissingSymbol(&'static str),

    /// The decoder reached an unexpected Huffman out-of-band marker.
    #[error("JBIG2Decode: unexpected Huffman OOB")]
    UnexpectedHuffmanOob,

    /// A CCITT/MMR-backed decode failed.
    #[error(transparent)]
    Ccitt(#[from] CcittDecodeError),
}

impl From<BitReaderError> for Jbig2Error {
    fn from(err: BitReaderError) -> Self {
        match err {
            BitReaderError::Truncated(message) => Self::Truncated(message),
            BitReaderError::Overflow(message) => Self::Overflow(message),
            _ => Self::InvalidState("bit reader error"),
        }
    }
}
