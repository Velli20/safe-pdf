//! Shared source location for errors identified by a byte offset.

use crate::{Jpeg2000Error, marker_reader::MarkerError, marker_site::MarkerSite};

/// A byte offset in the original input used for structural diagnostics.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct OffsetSite {
    offset: usize,
}

impl OffsetSite {
    /// Records the byte offset at which an error should be reported.
    pub(crate) fn new(offset: usize) -> Self {
        Self { offset }
    }

    /// Returns the recorded byte offset.
    pub(crate) fn offset(self) -> usize {
        self.offset
    }

    /// Advances the recorded position, or returns `None` on overflow.
    pub(crate) fn checked_advance(self, bytes: usize) -> Option<Self> {
        self.offset.checked_add(bytes).map(Self::new)
    }

    /// Associates a marker code with this position.
    pub(crate) fn marker(self, code: u16) -> MarkerSite {
        MarkerSite::new(self.offset, code)
    }

    /// Reports missing bytes in the named codestream structure.
    pub(crate) fn truncated(self, context: &'static str) -> Jpeg2000Error {
        Jpeg2000Error::Truncated {
            offset: self.offset,
            context,
        }
    }

    /// Rejects a value because the named structure is incomplete.
    pub(crate) fn reject_truncated<T>(self, context: &'static str) -> Result<T, Jpeg2000Error> {
        Err(self.truncated(context))
    }

    /// Reports a missing marker prefix or code at this offset.
    pub(crate) fn prefix<T>(self) -> Result<T, MarkerError> {
        Err(MarkerError::Prefix(self.offset))
    }

    /// Reports an incomplete marker length field at this offset.
    pub(crate) fn length<T>(self) -> Result<T, MarkerError> {
        Err(MarkerError::Length(self.offset))
    }

    /// Reports an incomplete marker payload at this offset.
    pub(crate) fn payload(self) -> MarkerError {
        MarkerError::Payload(self.offset)
    }
}
