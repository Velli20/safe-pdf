//! A byte supply spanning the pieces of one codeword segment.
//!
//! A code-block's codeword segment is written once by the encoder but reaches
//! the decoder in as many pieces as there are quality layers contributing to
//! it. The arithmetic coder and the raw bit reader both have to read across
//! those pieces as one stream, which is what this source provides without
//! copying any compressed byte.

use pdf_mq_coder::MqByteSource;

use crate::stuffed_bits::HeaderSource;

/// A forward byte supply over the pieces of one codeword segment.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ChainSource<'a> {
    chunks: &'a [&'a [u8]],
    position: usize,
}

impl<'a> ChainSource<'a> {
    /// Reads the pieces of one codeword segment in order.
    pub(crate) fn new(chunks: &'a [&'a [u8]]) -> Self {
        Self {
            chunks,
            position: 0,
        }
    }

    /// Returns the byte `skip` positions past the current one.
    fn byte_at(&self, skip: usize) -> Option<u8> {
        let mut remaining = self.position.checked_add(skip)?;
        for chunk in self.chunks {
            if remaining < chunk.len() {
                return chunk.get(remaining).copied();
            }
            remaining = remaining.checked_sub(chunk.len())?;
        }
        None
    }
}

impl MqByteSource for ChainSource<'_> {
    fn peek(&self) -> Option<u8> {
        self.byte_at(0)
    }

    fn peek_next(&self) -> Option<u8> {
        self.byte_at(1)
    }

    fn advance(&mut self) {
        self.position = self.position.saturating_add(1);
    }
}

impl HeaderSource for ChainSource<'_> {
    fn next_byte(&mut self) -> Option<u8> {
        let byte = self.byte_at(0)?;
        self.position = self.position.checked_add(1)?;
        Some(byte)
    }

    fn consumed(&self) -> usize {
        self.position
    }
}
