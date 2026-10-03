//! A logical byte stream assembled from several borrowed slices.
//!
//! A tile's compressed data is split across its tile-parts, and its packet
//! headers may be split again across PPM or PPT segments. Both are read as one
//! stream without copying the compressed bytes: only the list of slices is
//! owned, and it is charged against the caller's working-memory bound.

use crate::{
    Jpeg2000Error, offset_site::OffsetSite, stuffed_bits::HeaderSource, workspace::Workspace,
};

/// Bytes in the big-endian length fields read from a packed-header stream.
const LENGTH_BYTES: usize = 4;

/// An ordered list of borrowed slices read as one stream.
#[derive(Debug, Default)]
pub(crate) struct ChunkedBytes<'a> {
    chunks: Vec<&'a [u8]>,
    len: usize,
}

impl<'a> ChunkedBytes<'a> {
    /// Appends one slice to the end of the stream.
    ///
    /// # Errors
    ///
    /// Returns `LimitExceeded` when the chunk list passes the caller's
    /// working-memory bound, and `Overflow` if the stream becomes too long to
    /// address.
    pub(crate) fn push(
        &mut self,
        chunk: &'a [u8],
        workspace: &mut Workspace,
    ) -> Result<(), Jpeg2000Error> {
        if chunk.is_empty() {
            return Ok(());
        }
        self.len = self
            .len
            .checked_add(chunk.len())
            .ok_or(Jpeg2000Error::Overflow {
                context: "chunked stream length",
            })?;
        workspace.push(&mut self.chunks, chunk)
    }

    /// Returns the total number of bytes in the stream.
    pub(crate) fn len(&self) -> usize {
        self.len
    }

    /// Returns whether the stream carries no bytes.
    pub(crate) fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns the borrowed slices making up the stream, in order.
    pub(crate) fn chunks(&self) -> &[&'a [u8]] {
        &self.chunks
    }

    /// Returns a cursor positioned at the start of the stream.
    pub(crate) fn cursor(&self) -> ChunkCursor<'a, '_> {
        ChunkCursor {
            chunks: &self.chunks,
            chunk: 0,
            offset: 0,
            consumed: 0,
        }
    }

    /// Returns a cursor positioned `position` bytes into the stream.
    ///
    /// The cursor counts from zero again, so a caller that resumes reading
    /// tracks the absolute position itself.
    pub(crate) fn cursor_at(&self, position: usize) -> Option<ChunkCursor<'a, '_>> {
        let mut offset = position;
        for (chunk, slice) in self.chunks.iter().enumerate() {
            if offset <= slice.len() {
                return Some(ChunkCursor {
                    chunks: &self.chunks,
                    chunk,
                    offset,
                    consumed: 0,
                });
            }
            offset = offset.checked_sub(slice.len())?;
        }
        (offset == 0).then(|| self.cursor_at_end())
    }

    /// Returns a cursor positioned past the last chunk.
    fn cursor_at_end(&self) -> ChunkCursor<'a, '_> {
        ChunkCursor {
            chunks: &self.chunks,
            chunk: self.chunks.len(),
            offset: 0,
            consumed: 0,
        }
    }

    /// Borrows `len` bytes at `start` when they lie inside one chunk.
    ///
    /// Returns `None` when the range crosses a chunk boundary, which for
    /// compressed data means a codeword segment straddles two tile-parts.
    pub(crate) fn contiguous(&self, start: usize, len: usize) -> Option<&'a [u8]> {
        let mut position = start;
        for chunk in &self.chunks {
            if position < chunk.len() {
                return chunk.get(position..position.checked_add(len)?);
            }
            position = position.checked_sub(chunk.len())?;
        }
        (position == 0 && len == 0).then_some(&[])
    }

    /// Returns a stream over `len` bytes starting at `start`.
    ///
    /// # Errors
    ///
    /// Returns `Overflow` when the range lies outside the stream, and
    /// `LimitExceeded` when the new chunk list passes the caller's bound.
    pub(crate) fn slice(
        &self,
        start: usize,
        len: usize,
        workspace: &mut Workspace,
    ) -> Result<Self, Jpeg2000Error> {
        let overflow = || Jpeg2000Error::Overflow {
            context: "chunked stream range",
        };
        let end = start.checked_add(len).ok_or_else(overflow)?;
        if end > self.len {
            return Err(overflow());
        }
        let mut result = Self::default();
        let mut position = 0usize;
        for chunk in &self.chunks {
            let chunk_end = position.checked_add(chunk.len()).ok_or_else(overflow)?;
            if chunk_end > start && position < end {
                let from = start.saturating_sub(position);
                let to = end.saturating_sub(position).min(chunk.len());
                let part = chunk.get(from..to).ok_or_else(overflow)?;
                result.push(part, workspace)?;
            }
            position = chunk_end;
        }
        Ok(result)
    }
}

/// A forward cursor over a [`ChunkedBytes`] stream.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ChunkCursor<'a, 'stream> {
    chunks: &'stream [&'a [u8]],
    chunk: usize,
    offset: usize,
    consumed: usize,
}

impl ChunkCursor<'_, '_> {
    /// Returns the number of bytes read from the stream so far.
    pub(crate) fn position(&self) -> usize {
        self.consumed
    }

    /// Reads one big-endian four-byte length field.
    ///
    /// # Errors
    ///
    /// Returns `Truncated` when the stream ends inside the field.
    pub(crate) fn read_length(&mut self, site: OffsetSite) -> Result<u32, Jpeg2000Error> {
        let mut value = 0u32;
        for _ in 0..LENGTH_BYTES {
            let byte = self
                .next_byte()
                .ok_or_else(|| site.truncated("packed packet header length"))?;
            value = value.wrapping_shl(u8::BITS) | u32::from(byte);
        }
        Ok(value)
    }

    /// Skips `len` bytes of the stream.
    ///
    /// # Errors
    ///
    /// Returns `Truncated` when fewer than `len` bytes remain.
    pub(crate) fn skip(&mut self, len: usize, site: OffsetSite) -> Result<(), Jpeg2000Error> {
        for _ in 0..len {
            self.next_byte()
                .ok_or_else(|| site.truncated("packed packet header"))?;
        }
        Ok(())
    }
}

impl HeaderSource for ChunkCursor<'_, '_> {
    fn next_byte(&mut self) -> Option<u8> {
        loop {
            let chunk = self.chunks.get(self.chunk)?;
            if let Some(byte) = chunk.get(self.offset) {
                self.offset = self.offset.checked_add(1)?;
                self.consumed = self.consumed.checked_add(1)?;
                return Some(*byte);
            }
            self.chunk = self.chunk.checked_add(1)?;
            self.offset = 0;
        }
    }

    fn consumed(&self) -> usize {
        self.consumed
    }
}
