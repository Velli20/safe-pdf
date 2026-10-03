//! Typed failures at JPEG 2000 container, codestream, and tile boundaries.
//!
//! Every variant owns its diagnostics and carries the byte offset of the
//! structure that failed, so an error can outlive the borrowed input.

use pdf_mq_coder::MqError;
use pdf_utils::error::BitReaderError;
use thiserror::Error;

use crate::box_reader::{BoxError, BoxKind};
use crate::coding::CodingError;
use crate::jp2::ContainerError;
use crate::marker_reader::MarkerError;
use crate::quantization::QuantizationError;

/// A resource bounded by [`crate::DecoderLimits`] for untrusted PDF images.
///
/// The variants let callers report a precise limit instead of interpreting an
/// allocation failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Resource {
    /// Compressed bytes supplied by the caller.
    InputBytes,
    /// Pixels in the reference image grid.
    Pixels,
    /// Components declared by the SIZ marker.
    Components,
    /// Tiles implied by the SIZ image and tile grids.
    Tiles,
    /// Temporary bytes held while reconstructing a tile.
    WorkingBytes,
}

impl Resource {
    /// Rejects a request that exceeds the caller's bound for this resource.
    pub(crate) fn check(self, requested: u64, limit: u64) -> Result<(), Jpeg2000Error> {
        if requested > limit {
            return Err(Jpeg2000Error::LimitExceeded {
                resource: self,
                requested,
                limit,
            });
        }
        Ok(())
    }

    /// Rejects a byte count that exceeds a bound expressed in `usize`.
    pub(crate) fn check_bytes(self, requested: usize, limit: usize) -> Result<(), Jpeg2000Error> {
        self.check(
            u64::try_from(requested).unwrap_or(u64::MAX),
            u64::try_from(limit).unwrap_or(u64::MAX),
        )
    }
}

/// A JPEG 2000 Part 1 or JP2 decoding failure.
///
/// Structural errors distinguish malformed input from unsupported syntax and
/// unfinished implementation.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Jpeg2000Error {
    /// A JP2-family box could not be framed safely.
    #[error(transparent)]
    Box(#[from] BoxError),
    /// Coding or quantization defaults are malformed.
    #[error(transparent)]
    Coding(#[from] CodingError),
    /// A JP2-family container violates its box structure.
    #[error(transparent)]
    Container(#[from] ContainerError),
    /// A codestream marker could not be framed safely.
    #[error(transparent)]
    Marker(#[from] MarkerError),
    /// Quantization parameters are malformed.
    #[error(transparent)]
    Quantization(#[from] QuantizationError),
    /// The MQ arithmetic coder could not produce a decision.
    #[error("arithmetic decoding failed: {0}")]
    Arithmetic(#[from] MqError),
    /// A checked byte read failed while interpreting a validated marker body.
    #[error("codestream field read failed: {0}")]
    BitRead(#[from] BitReaderError),
    /// A box, marker segment, or packet ended before its declared length.
    #[error("truncated {context} at byte offset {offset}")]
    Truncated {
        /// Byte offset relative to the input slice.
        offset: usize,
        /// The structure whose bytes were missing.
        context: &'static str,
    },

    /// A JP2 box has an invalid signature, length, or required value.
    #[error("invalid JP2 box {kind} at byte offset {offset}: {reason}")]
    InvalidBox {
        /// Byte offset relative to the input slice.
        offset: usize,
        /// Type of the invalid box.
        kind: BoxKind,
        /// Static description of the violated box rule.
        reason: &'static str,
    },

    /// A codestream marker segment contains invalid syntax.
    #[error("invalid marker {marker:#06x} at byte offset {offset}: {reason}")]
    InvalidMarker {
        /// Byte offset relative to the input slice.
        offset: usize,
        /// Two-byte marker code.
        marker: u16,
        /// Static description of the violated marker rule.
        reason: &'static str,
    },

    /// A marker appears outside its allowed main-header or tile-part position.
    #[error("marker {marker:#06x} is out of order at byte offset {offset}")]
    MarkerOrder {
        /// Byte offset relative to the input slice.
        offset: usize,
        /// Two-byte marker code.
        marker: u16,
    },

    /// The codestream requests a feature outside this Part 1 decoder's scope.
    #[error("unsupported JPEG 2000 feature: {feature}")]
    UnsupportedFeature {
        /// Name of the unsupported coding or container feature.
        feature: &'static str,
    },

    /// The SIZ marker declares a profile the decoder cannot interpret.
    #[error("unsupported JPEG 2000 profile {profile}")]
    UnsupportedProfile {
        /// Raw Rsiz profile value from the SIZ marker.
        profile: u16,
    },

    /// Input metadata exceeds a caller-supplied resource limit.
    #[error("{resource:?} limit exceeded: requested {requested}, limit {limit}")]
    LimitExceeded {
        /// Resource subject to the bound.
        resource: Resource,
        /// Amount requested by the stream.
        requested: u64,
        /// Maximum accepted by the caller.
        limit: u64,
    },

    /// Size arithmetic or a lossless integer conversion overflowed.
    #[error("integer overflow while computing {context}")]
    Overflow {
        /// The size or offset that could not be represented.
        context: &'static str,
    },

    /// A caller-provided tile sink could not accept reconstructed samples.
    #[error("tile output failed: {message}")]
    Output {
        /// Owned sink diagnostic, safe to retain beyond a tile callback.
        message: String,
    },

    /// A tile-part or tile is missing, repeated, or out of order.
    #[error("invalid tile-part {part} of tile {tile} at byte offset {offset}: {reason}")]
    InvalidTilePart {
        /// Byte offset relative to the input slice.
        offset: usize,
        /// Zero-based SIZ tile index from Isot.
        tile: u32,
        /// Zero-based tile-part index from TPsot.
        part: u16,
        /// Static description of the violated tile-part rule.
        reason: &'static str,
    },
}
