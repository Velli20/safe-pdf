//! Shared source location for errors about codestream markers.

use crate::{Jpeg2000Error, marker_reader::MarkerError, offset_site::OffsetSite};

/// The byte offset and code of one codestream marker.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MarkerSite {
    offset: usize,
    code: u16,
}

impl MarkerSite {
    /// Records a marker's position and two-byte code for later diagnostics.
    pub(crate) fn new(offset: usize, code: u16) -> Self {
        Self { offset, code }
    }

    /// Returns the marker's byte offset in the original input.
    pub(crate) fn offset(self) -> usize {
        self.offset
    }

    /// Returns the marker's position without its code.
    pub(crate) fn offset_site(self) -> OffsetSite {
        OffsetSite::new(self.offset)
    }

    /// Returns the marker's raw two-byte code.
    pub(crate) fn code(self) -> u16 {
        self.code
    }

    /// Reports invalid marker syntax or a marker-related consistency failure.
    pub(crate) fn invalid(self, reason: &'static str) -> Jpeg2000Error {
        Jpeg2000Error::InvalidMarker {
            offset: self.offset,
            marker: self.code,
            reason,
        }
    }

    /// Rejects a value because this marker is invalid.
    pub(crate) fn reject_invalid<T>(self, reason: &'static str) -> Result<T, Jpeg2000Error> {
        Err(self.invalid(reason))
    }

    /// Reports this marker in a disallowed position.
    pub(crate) fn out_of_order<T>(self) -> Result<T, Jpeg2000Error> {
        Err(Jpeg2000Error::MarkerOrder {
            offset: self.offset,
            marker: self.code,
        })
    }

    /// Reports a code that cannot start a marker segment.
    pub(crate) fn invalid_code<T>(self) -> Result<T, MarkerError> {
        Err(MarkerError::Code {
            offset: self.offset,
            code: self.code,
        })
    }

    /// Reports a marker segment shorter than its length field.
    pub(crate) fn short_segment<T>(self) -> Result<T, MarkerError> {
        Err(MarkerError::ShortSegment {
            offset: self.offset,
            code: self.code,
        })
    }
}
