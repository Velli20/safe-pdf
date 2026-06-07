//! Bit-reading helper for JBIG2 arithmetic-coded integers.
//!
//! ITU-T T.88 / ISO/IEC 14492 Annex A.2 and Annex A.3 both read one
//! arithmetic-coded bit at a time while updating a context-tree index. This
//! module isolates that tree maintenance for direct unit testing.

use crate::arith_decoder::{decoder::JBig2ArithDecoder, integer::JBig2ArithIntegerContext};
use crate::error::Jbig2Error;

/// Root context index for the Annex A.2 and Annex A.3 context trees.
const CONTEXT_TREE_ROOT: usize = 1;

/// First Annex A.2 integer context index that uses the rolling 9-bit window.
const INTEGER_ROLLING_CONTEXT_ROOT: usize = 256;

/// Mask for the lower 9 bits retained by Annex A.2 integer contexts.
const INTEGER_CONTEXT_WINDOW_MASK: usize = 511;

/// Source context tree for an arithmetic bit reader.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BitContextSource {
    /// Annex A.2 arithmetic integer contexts.
    Integer(JBig2ArithIntegerContext),
    /// Annex A.3 IAID contexts.
    Iaid,
}

/// Reads bits and updates Annex A.2/A.3 arithmetic context indices.
pub(super) struct ArithmeticBitReader<'decoder, 'stream, 'data> {
    decoder: &'decoder mut JBig2ArithDecoder<'stream, 'data>,
    source: BitContextSource,
    prev: usize,
}

impl<'decoder, 'stream, 'data> ArithmeticBitReader<'decoder, 'stream, 'data> {
    /// Create a bit reader rooted at the Annex A.2/A.3 initial context index.
    pub(super) fn new(
        decoder: &'decoder mut JBig2ArithDecoder<'stream, 'data>,
        source: BitContextSource,
    ) -> Self {
        Self {
            decoder,
            source,
            prev: CONTEXT_TREE_ROOT,
        }
    }

    /// Read `len` Annex A.2 integer bits and update the integer context index.
    pub(super) fn read_integer_bits(&mut self, len: u8) -> Result<u32, Jbig2Error> {
        let mut value = 0u32;
        for _ in 0..len {
            let bit = self.decode_current_context()?;
            self.advance_integer_context(bit)?;
            value = value
                .checked_shl(1)
                .and_then(|current| current.checked_add(u32::from(bit)))
                .ok_or(Jbig2Error::Overflow("arithmetic integer value overflow"))?;
        }
        Ok(value)
    }

    /// Read the raw Annex A.3 IAID context-tree value before masking.
    pub(super) fn read_iaid_raw(&mut self, code_length: u8) -> Result<u32, Jbig2Error> {
        for _ in 0..code_length {
            let bit = usize::from(self.decode_current_context()?);
            self.prev = self
                .prev
                .checked_shl(1)
                .and_then(|value| value.checked_add(bit))
                .ok_or(Jbig2Error::Overflow("IAID value overflow"))?;
        }

        u32::try_from(self.prev).map_err(|_| Jbig2Error::Overflow("IAID value overflow"))
    }

    /// Return the current Annex A.2/A.3 context index.
    #[cfg(test)]
    pub(super) fn context_index(&self) -> usize {
        self.prev
    }

    /// Decode one bit from the current context-tree index.
    fn decode_current_context(&mut self) -> Result<u8, Jbig2Error> {
        self.decoder
            .decode_selected_context(&self.source, self.prev)
    }

    /// Advance the Annex A.2 integer context tree after one decoded bit.
    fn advance_integer_context(&mut self, bit: u8) -> Result<(), Jbig2Error> {
        let bit = usize::from(bit);
        self.prev = if self.prev < INTEGER_ROLLING_CONTEXT_ROOT {
            self.prev
                .checked_shl(1)
                .and_then(|value| value.checked_add(bit))
                .ok_or(Jbig2Error::Overflow("arithmetic integer context overflow"))?
        } else {
            let next = self
                .prev
                .checked_shl(1)
                .and_then(|value| value.checked_add(bit))
                .ok_or(Jbig2Error::Overflow("arithmetic integer context overflow"))?;
            (next & INTEGER_CONTEXT_WINDOW_MASK) | INTEGER_ROLLING_CONTEXT_ROOT
        };
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{ArithmeticBitReader, BitContextSource, INTEGER_ROLLING_CONTEXT_ROOT};
    use crate::arith_decoder::{JBig2ArithDecoder, JBig2ArithIntegerContext};
    use crate::error::Jbig2Error;
    use pdf_utils::BitReader;

    #[test]
    fn integer_context_advances_as_binary_tree_below_rolling_root() -> Result<(), Jbig2Error> {
        let data = [0x00u8, 0x00u8];
        let mut stream = BitReader::new(&data);
        let mut decoder = JBig2ArithDecoder::new(&mut stream);
        let mut bits = ArithmeticBitReader::new(
            &mut decoder,
            BitContextSource::Integer(JBig2ArithIntegerContext::TextDeltaT),
        );

        bits.advance_integer_context(1)?;
        assert_eq!(bits.context_index(), 3);
        bits.advance_integer_context(0)?;
        assert_eq!(bits.context_index(), 6);
        Ok(())
    }

    #[test]
    fn integer_context_retains_rolling_window_after_root() -> Result<(), Jbig2Error> {
        let data = [0x00u8, 0x00u8];
        let mut stream = BitReader::new(&data);
        let mut decoder = JBig2ArithDecoder::new(&mut stream);
        let mut bits = ArithmeticBitReader::new(
            &mut decoder,
            BitContextSource::Integer(JBig2ArithIntegerContext::TextDeltaT),
        );

        while bits.context_index() < INTEGER_ROLLING_CONTEXT_ROOT {
            bits.advance_integer_context(1)?;
        }
        bits.advance_integer_context(0)?;

        assert!(bits.context_index() >= INTEGER_ROLLING_CONTEXT_ROOT);
        assert!(bits.context_index() < INTEGER_ROLLING_CONTEXT_ROOT.saturating_mul(2));
        Ok(())
    }
}
