//! JBIG2 arithmetic decoder state and facade.
//!
//! This module owns the decoder state and construction path for ITU-T T.88 /
//! ISO/IEC 14492 Annex A arithmetic streams. The actual Annex A.1 coding
//! step, byte input, context pools, Annex A.2 integers, and Annex A.3 IAID
//! procedures are split into sibling modules.

use crate::arith_decoder::{
    byte_input::ARITHMETIC_BYTE_FALLBACK,
    coding::{DEFAULT_INTERVAL, INITIAL_CODE_REGISTER_SHIFT, POST_BYTE_IN_CODE_SHIFT},
    context::JBig2ArithCtx,
};
use crate::error::Jbig2Error;
use pdf_utils::BitReader;

/// JBIG2 arithmetic decoder state for T.88 Annex A streams.
///
/// The decoder keeps the Annex A.1 code and interval registers, byte-input
/// marker state, and all adaptive context pools needed by generic-region,
/// integer, and IAID decoding procedures.
#[derive(Debug)]
pub(crate) struct JBig2ArithDecoder<'stream, 'data> {
    /// Shared JBIG2 segment bit reader that supplies arithmetic bytes.
    pub(super) stream: &'stream mut BitReader<'data>,
    /// Optional exclusive byte position limiting decoding to the current segment.
    pub(super) byte_limit: Option<usize>,
    /// Whether decoding was initialized without any real arithmetic byte.
    pub(super) complete: bool,
    /// Most recently loaded byte for Annex A.1 `BYTEIN`.
    pub(super) current_byte: u8,
    /// Annex A.1 code register `C`.
    pub(super) code: u32,
    /// Annex A.1 interval register `A`.
    pub(super) interval: u32,
    /// Annex A.1 byte-input bit counter `CT`.
    pub(super) bit_count: u32,
    /// Generic-region contexts used by T.88 section 6.2.5.7.
    pub(super) generic_region_contexts: Vec<JBig2ArithCtx>,
    /// `IADT` contexts for text-region delta `T` arithmetic integers.
    pub(super) iadt_contexts: Vec<JBig2ArithCtx>,
    /// `IAFS` contexts for first-symbol `S` arithmetic integers.
    pub(super) iafs_contexts: Vec<JBig2ArithCtx>,
    /// `IAIT` contexts for text-instance `T` arithmetic integers.
    pub(super) iait_contexts: Vec<JBig2ArithCtx>,
    /// `IADS` contexts for text delta `S` arithmetic integers.
    pub(super) iads_contexts: Vec<JBig2ArithCtx>,
    /// `IADH` contexts for symbol-height delta arithmetic integers.
    pub(super) iadh_contexts: Vec<JBig2ArithCtx>,
    /// `IADW` contexts for symbol-width delta arithmetic integers.
    pub(super) iadw_contexts: Vec<JBig2ArithCtx>,
    /// `IAEX` contexts for symbol export run-length arithmetic integers.
    pub(super) iaex_contexts: Vec<JBig2ArithCtx>,
    /// `IAAI` contexts for refinement aggregate instance counts.
    pub(super) iaai_contexts: Vec<JBig2ArithCtx>,
    /// `IARDW` contexts for refinement width deltas.
    pub(super) iardw_contexts: Vec<JBig2ArithCtx>,
    /// `IARDH` contexts for refinement height deltas.
    pub(super) iardh_contexts: Vec<JBig2ArithCtx>,
    /// `IARDX` contexts for refinement x deltas.
    pub(super) iardx_contexts: Vec<JBig2ArithCtx>,
    /// `IARDY` contexts for refinement y deltas.
    pub(super) iardy_contexts: Vec<JBig2ArithCtx>,
    /// `IARI` contexts for refinement instance flags.
    pub(super) iari_contexts: Vec<JBig2ArithCtx>,
    /// Annex A.3 IAID context tree.
    pub(super) iaid_contexts: Vec<JBig2ArithCtx>,
    /// Code length used to size the current Annex A.3 IAID context tree.
    pub(super) iaid_code_length: Option<u8>,
}

impl<'stream, 'data> JBig2ArithDecoder<'stream, 'data> {
    /// Create a JBIG2 arithmetic decoder for the remaining stream bytes.
    ///
    /// This initializes the Annex A.1 code and interval registers from the
    /// current byte position of `stream`.
    pub(crate) fn new(stream: &'stream mut BitReader<'data>) -> Self {
        Self::new_with_limit(stream, None)
    }

    /// Create a JBIG2 arithmetic decoder limited to `end_byte_pos`.
    ///
    /// T.88 arithmetic decoding is byte-oriented; callers use this constructor
    /// when the containing segment has a known end byte. Reads at or beyond the
    /// limit synthesize `0xff` in the same way as end-of-stream peeking.
    pub(crate) fn new_until(
        stream: &'stream mut BitReader<'data>,
        end_byte_pos: usize,
    ) -> Result<Self, Jbig2Error> {
        stream
            .remaining_from_byte_until(end_byte_pos)
            .ok_or(Jbig2Error::Truncated("arithmetic stream"))?;
        Ok(Self::new_with_limit(stream, Some(end_byte_pos)))
    }

    /// Initialize Annex A.1 decoder state with an optional segment byte limit.
    fn new_with_limit(stream: &'stream mut BitReader<'data>, byte_limit: Option<usize>) -> Self {
        let has_initial_byte = Self::has_current_byte(stream, byte_limit);
        let current_byte = Self::peek_byte_or(stream, byte_limit, ARITHMETIC_BYTE_FALLBACK);
        let mut decoder = Self {
            stream,
            byte_limit,
            complete: !has_initial_byte,
            current_byte,
            code: u32::from(current_byte ^ ARITHMETIC_BYTE_FALLBACK) << INITIAL_CODE_REGISTER_SHIFT,
            interval: DEFAULT_INTERVAL,
            bit_count: 0,
            generic_region_contexts: Vec::new(),
            iadt_contexts: Vec::new(),
            iafs_contexts: Vec::new(),
            iait_contexts: Vec::new(),
            iads_contexts: Vec::new(),
            iadh_contexts: Vec::new(),
            iadw_contexts: Vec::new(),
            iaex_contexts: Vec::new(),
            iaai_contexts: Vec::new(),
            iardw_contexts: Vec::new(),
            iardh_contexts: Vec::new(),
            iardx_contexts: Vec::new(),
            iardy_contexts: Vec::new(),
            iari_contexts: Vec::new(),
            iaid_contexts: Vec::new(),
            iaid_code_length: None,
        };
        decoder.byte_in();
        decoder.code = decoder.code.wrapping_shl(POST_BYTE_IN_CODE_SHIFT);
        decoder.bit_count = decoder.bit_count.saturating_sub(POST_BYTE_IN_CODE_SHIFT);
        decoder
    }
}

#[cfg(test)]
mod tests {
    use crate::arith_decoder::{JBig2ArithDecoder, JBig2ArithIntegerContext};
    use crate::error::Jbig2Error;
    use pdf_utils::BitReader;

    fn decode_integer_fixture(data: &[u8]) -> Result<Option<i32>, Jbig2Error> {
        let mut reader = BitReader::new(data);
        let mut decoder = JBig2ArithDecoder::new(&mut reader);
        decoder.decode_integer(JBig2ArithIntegerContext::TextDeltaT)
    }

    fn fixture_bytes(value: usize, len: usize) -> Result<Vec<u8>, Jbig2Error> {
        let value = u32::try_from(value).map_err(|_| Jbig2Error::Overflow("fixture value"))?;
        let bytes = value.to_be_bytes();
        match len {
            2 => Ok(bytes.iter().skip(2).copied().collect()),
            3 => Ok(bytes.iter().skip(1).copied().collect()),
            _ => Err(Jbig2Error::InvalidState("fixture length")),
        }
    }

    fn fixture_search_limit(len: usize) -> Result<usize, Jbig2Error> {
        let bit_count = len
            .checked_mul(8)
            .and_then(|value| u32::try_from(value).ok())
            .ok_or(Jbig2Error::Overflow("fixture search limit"))?;
        1usize
            .checked_shl(bit_count)
            .ok_or(Jbig2Error::Overflow("fixture search limit"))
    }

    fn find_integer_fixture(target: Option<i32>) -> Result<Vec<u8>, Jbig2Error> {
        for len in 2..=3usize {
            let max = fixture_search_limit(len)?;
            for value in 0..max {
                let bytes = fixture_bytes(value, len)?;
                if decode_integer_fixture(&bytes) == Ok(target) {
                    return Ok(bytes);
                }
            }
        }
        Err(Jbig2Error::InvalidState("integer fixture"))
    }

    fn find_iaid_fixture(target: u32, code_length: u8) -> Result<Vec<u8>, Jbig2Error> {
        for len in 2..=3usize {
            let max = fixture_search_limit(len)?;
            for value in 0..max {
                let bytes = fixture_bytes(value, len)?;
                let mut reader = BitReader::new(&bytes);
                let mut decoder = JBig2ArithDecoder::new(&mut reader);
                if decoder.decode_iaid(code_length) == Ok(target) {
                    return Ok(bytes);
                }
            }
        }
        Err(Jbig2Error::InvalidState("IAID fixture"))
    }

    #[test]
    fn decode_arith_integer_handles_positive_and_negative_values() -> Result<(), Jbig2Error> {
        let positive_one = find_integer_fixture(Some(1))?;
        let negative_one = find_integer_fixture(Some(-1))?;
        assert_eq!(decode_integer_fixture(&positive_one), Ok(Some(1)));
        assert_eq!(decode_integer_fixture(&negative_one), Ok(Some(-1)));
        Ok(())
    }

    #[test]
    fn decode_arith_integer_returns_none_for_oob_marker() -> Result<(), Jbig2Error> {
        let oob = find_integer_fixture(None)?;
        assert_eq!(decode_integer_fixture(&oob), Ok(None));
        Ok(())
    }

    #[test]
    fn decode_arith_integer_truncated_stream_is_typed_error() {
        let err = decode_integer_fixture(&[]).expect_err("expected error");
        assert_eq!(err, Jbig2Error::Truncated("arithmetic stream"));
    }

    #[test]
    fn decode_iaid_reads_symbol_identifier() -> Result<(), Jbig2Error> {
        let bytes = find_iaid_fixture(5, 3)?;
        let mut reader = BitReader::new(&bytes);
        let mut decoder = JBig2ArithDecoder::new(&mut reader);
        let value = decoder.decode_iaid(3).expect("value");
        assert_eq!(value, 5);
        Ok(())
    }
}
