//! IAID decoding for JBIG2 arithmetic streams.
//!
//! ITU-T T.88 / ISO/IEC 14492 Annex A.3 defines IAID as a fixed-width
//! arithmetic-coded symbol identifier with a context tree sized by code length.

use crate::arith_decoder::{
    bit_reader::{ArithmeticBitReader, BitContextSource},
    decoder::JBig2ArithDecoder,
};
use crate::error::Jbig2Error;

/// Maximum IAID width that can be represented by a direct `(1 << width) - 1` mask.
const IAID_MAX_MASKED_CODE_LENGTH: u8 = 31;

/// Mask used for IAID widths of 31 bits or greater.
const IAID_MAX_MASKED_VALUE: u32 = 0x7fff_ffff;

impl JBig2ArithDecoder<'_, '_> {
    /// Decode a JBIG2 IAID using T.88 Annex A.3.
    ///
    /// The returned value is the decoded symbol identifier, masked to the
    /// effective IAID width defined by the procedure.
    pub(crate) fn decode_iaid(&mut self, code_length: u8) -> Result<u32, Jbig2Error> {
        self.ensure_iaid_contexts(code_length)?;
        let mut bits = ArithmeticBitReader::new(self, BitContextSource::Iaid);
        let raw = bits.read_iaid_raw(code_length)?;
        if code_length < IAID_MAX_MASKED_CODE_LENGTH {
            Ok(raw & iaid_mask(code_length)?)
        } else {
            Ok(raw & IAID_MAX_MASKED_VALUE)
        }
    }

    /// Ensure the T.88 Annex A.3 IAID context tree matches `code_length`.
    pub(super) fn ensure_iaid_contexts(&mut self, code_length: u8) -> Result<(), Jbig2Error> {
        if self.iaid_code_length == Some(code_length) && !self.iaid_contexts.is_empty() {
            return Ok(());
        }

        let bit_count = u32::from(code_length).saturating_add(1);
        let len = 1usize
            .checked_shl(bit_count)
            .ok_or(Jbig2Error::Overflow("IAID context length overflow"))?;
        Self::ensure_contexts(&mut self.iaid_contexts, len, "IAID contexts")?;
        self.iaid_code_length = Some(code_length);
        Ok(())
    }
}

/// Return the Annex A.3 value mask for an IAID code length below 31 bits.
fn iaid_mask(code_length: u8) -> Result<u32, Jbig2Error> {
    if code_length == 0 {
        return Ok(0);
    }
    1u32.checked_shl(u32::from(code_length))
        .and_then(|value| value.checked_sub(1))
        .ok_or(Jbig2Error::Overflow("IAID mask overflow"))
}

#[cfg(test)]
mod tests {
    use crate::arith_decoder::JBig2ArithDecoder;
    use crate::error::Jbig2Error;
    use pdf_utils::BitReader;

    #[test]
    fn iaid_contexts_reset_when_code_length_changes() -> Result<(), Jbig2Error> {
        let data = [0x00u8, 0x00u8];
        let mut stream = BitReader::new(&data);
        let mut decoder = JBig2ArithDecoder::new(&mut stream);

        decoder.ensure_iaid_contexts(2)?;
        assert_eq!(decoder.iaid_contexts.len(), 8);
        decoder.ensure_iaid_contexts(3)?;
        assert_eq!(decoder.iaid_contexts.len(), 16);
        Ok(())
    }
}
