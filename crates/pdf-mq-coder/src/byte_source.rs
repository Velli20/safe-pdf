//! The byte supply an MQ decoder draws its compressed data from.

/// Supplies the bytes an MQ arithmetic decoder consumes.
///
/// `BYTEIN` looks at the current byte and the one after it before advancing,
/// so a source exposes both without consuming either. Past the end of the
/// available data a source reports `None`, and the decoder substitutes the
/// `0xff` padding both standards prescribe.
pub trait MqByteSource {
    /// Returns the byte at the current position, if one is available.
    fn peek(&self) -> Option<u8>;

    /// Returns the byte after the current position, if one is available.
    fn peek_next(&self) -> Option<u8>;

    /// Advances the current position by one byte.
    fn advance(&mut self);
}

/// An [`MqByteSource`] over one contiguous codeword segment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SliceSource<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> SliceSource<'a> {
    /// Supplies arithmetic bytes from the start of a borrowed slice.
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }

    /// Returns the number of bytes the decoder has moved past.
    pub fn position(&self) -> usize {
        self.position
    }
}

impl MqByteSource for SliceSource<'_> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.position).copied()
    }

    fn peek_next(&self) -> Option<u8> {
        self.bytes.get(self.position.checked_add(1)?).copied()
    }

    fn advance(&mut self) {
        self.position = self.position.saturating_add(1).min(self.bytes.len());
    }
}
